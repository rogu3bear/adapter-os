use super::attachments::{
    reset_file_input_value, selected_file_from_event, set_timeout_simple,
    validate_attach_upload_file,
};
use super::composer::ChatComposerPanel;
use super::formatters::{
    attach_reason_detail, attach_reason_label, citation_page_span_label, degraded_kind_label,
    degraded_level_class, degraded_level_label, format_token_display, prominent_degraded_title,
    short_adapter_label, trust_summary_label,
};
use super::status_banners::ChatStreamAndPausedStatus;
use super::target_selector::ChatTargetSelector;
use super::workspace::{
    map_session_confirmation_error, AttachMode, SessionConfirmationState,
    CHAT_SCROLL_BOTTOM_THRESHOLD_PX, DOCUMENT_UPLOAD_MAX_FILE_SIZE, MAX_URL_PROMPT_LENGTH,
};
#[cfg(target_arch = "wasm32")]
use crate::api::{api_base_url, ApiClient};
use crate::components::inference_guidance::guidance_for;
use crate::components::status_center::use_status_center;
use crate::components::{
    use_is_tablet_or_smaller, AdapterHeat, AdapterMagnet, Badge, BadgeVariant, Button, ButtonLink,
    ButtonSize, ButtonVariant, ChatAdaptersRegion, Checkbox, Dialog, Markdown, MarkdownStream,
    Spinner, SuggestedAdapterView, Textarea, TraceButton, TracePanel,
};
use crate::hooks::{use_system_status, LoadingState};
use crate::signals::{use_chat, use_settings, ChatSessionsManager, ChatTarget, StreamNoticeTone};
#[cfg(target_arch = "wasm32")]
use crate::utils::status_display_with_raw;
use adapteros_api_types::inference::{
    AdapterAttachment, DegradedNotice, DegradedNoticeKind, DegradedNoticeLevel,
};
use adapteros_api_types::training::ChatMessageInput;
use adapteros_api_types::InferenceReadyState;
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use std::collections::BTreeSet;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatDrawerKind {
    Evidence,
    Context,
}

impl ChatDrawerKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Evidence => "Evidence",
            Self::Context => "Context",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatMobileLane {
    Conversation,
    Evidence,
    Context,
}

#[derive(Debug, Clone)]
struct ChatSessionLayoutVm {
    status_label: String,
    status_variant: BadgeVariant,
    context_model_label: String,
    context_adapter_label: String,
    mode_label: String,
    mode_variant: BadgeVariant,
    trust_summary: String,
    latest_run_url: Option<String>,
    latest_replay_url: Option<String>,
    latest_signed_log_url: Option<String>,
    degraded_count: usize,
    meaning_change_count: usize,
}

#[derive(Clone)]
struct MessageCallbacks {
    jump_to_latest: Callback<()>,
}

fn build_message_callbacks(jump_to_latest: Callback<()>) -> MessageCallbacks {
    MessageCallbacks { jump_to_latest }
}

#[derive(Clone)]
struct DrawerCallbacks {
    toggle_evidence: Callback<()>,
    toggle_context: Callback<()>,
    close_drawer: Callback<()>,
    set_lane_conversation: Callback<()>,
    set_lane_evidence: Callback<()>,
    set_lane_context: Callback<()>,
}

fn build_drawer_callbacks(
    active_drawer: RwSignal<Option<ChatDrawerKind>>,
    mobile_lane: RwSignal<ChatMobileLane>,
) -> DrawerCallbacks {
    let toggle_evidence = Callback::new(move |_: ()| {
        active_drawer.update(|drawer| {
            *drawer = if matches!(*drawer, Some(ChatDrawerKind::Evidence)) {
                None
            } else {
                Some(ChatDrawerKind::Evidence)
            };
        });
    });
    let toggle_context = Callback::new(move |_: ()| {
        active_drawer.update(|drawer| {
            *drawer = if matches!(*drawer, Some(ChatDrawerKind::Context)) {
                None
            } else {
                Some(ChatDrawerKind::Context)
            };
        });
    });
    let close_drawer = Callback::new(move |_: ()| {
        active_drawer.set(None);
    });
    let set_lane_conversation = Callback::new(move |_: ()| {
        mobile_lane.set(ChatMobileLane::Conversation);
    });
    let set_lane_evidence = Callback::new(move |_: ()| {
        mobile_lane.set(ChatMobileLane::Evidence);
    });
    let set_lane_context = Callback::new(move |_: ()| {
        mobile_lane.set(ChatMobileLane::Context);
    });
    DrawerCallbacks {
        toggle_evidence,
        toggle_context,
        close_drawer,
        set_lane_conversation,
        set_lane_evidence,
        set_lane_context,
    }
}

#[derive(Clone)]
struct SessionNoticeCallbacks {
    retry_session_confirmation: Callback<()>,
    clear_error: Callback<()>,
}

fn build_session_notice_callbacks(
    retry_session_confirmation: Callback<()>,
    clear_error: Callback<()>,
) -> SessionNoticeCallbacks {
    SessionNoticeCallbacks {
        retry_session_confirmation,
        clear_error,
    }
}

#[component]
fn ChatConversationMessageItem(
    msg_id: String,
    active_trace: RwSignal<Option<String>>,
) -> impl IntoView {
    let (chat_state, _) = use_chat();
    let compact_layout = use_is_tablet_or_smaller();
    let lookup_id = msg_id.clone();

    let message = Memo::new(move |_| {
        chat_state
            .try_get()
            .unwrap_or_default()
            .messages
            .iter()
            .find(|m| m.id == lookup_id)
            .cloned()
    });

    let streaming_content = Signal::derive(move || {
        message
            .try_get()
            .flatten()
            .filter(|m| m.role == "assistant" && m.is_streaming)
            .map(|m| m.content)
            .unwrap_or_default()
    });

    view! {
        {move || {
            message.try_get().flatten().map(|msg| {
                let is_user = msg.role == "user";
                let is_system = msg.role == "system";
                let is_streaming = msg.is_streaming;
                let trace_id = msg.trace_id.clone();
                let latency_ms = msg.latency_ms;
                let token_count = msg.token_count;
                let prompt_tokens = msg.prompt_tokens;
                let completion_tokens = msg.completion_tokens;
                let citations = msg.citations.clone().unwrap_or_default();
                let document_links = msg.document_links.clone().unwrap_or_default();
                let adapters_used = msg.adapters_used.clone().unwrap_or_default();
                let unavailable_pinned_adapters =
                    msg.unavailable_pinned_adapters.clone().unwrap_or_default();
                let pinned_routing_fallback = msg.pinned_routing_fallback.clone();
                let fallback_triggered = msg.fallback_triggered;
                let fallback_backend = msg.fallback_backend.clone();
                let adapter_attachments = msg.adapter_attachments.clone();
                let degraded_notices = msg.degraded_notices.clone();
                let citation_count = citations.len();
                let document_link_count = document_links.len();
                let has_trust_details = !adapter_attachments.is_empty()
                    || !degraded_notices.is_empty()
                    || !unavailable_pinned_adapters.is_empty()
                    || pinned_routing_fallback.is_some()
                    || fallback_triggered
                    || citation_count > 0
                    || document_link_count > 0
                    || !adapters_used.is_empty();
                let critical_notices: Vec<DegradedNotice> = degraded_notices
                    .iter()
                    .filter(|notice| {
                        notice.meaning_changed && notice.level == DegradedNoticeLevel::Critical
                    })
                    .cloned()
                    .collect();
                let trust_state = if citation_count > 0 || document_link_count > 0 {
                    "chat-provenance-cited"
                } else {
                    "chat-provenance-none"
                };
                let role_label = if is_user {
                    "You"
                } else if is_system {
                    "System"
                } else {
                    "Assistant"
                };

                view! {
                    <div class=format!(
                        "flex {}",
                        if is_user {
                            "justify-end"
                        } else if is_system {
                            "justify-center"
                        } else {
                            "justify-start"
                        }
                    )>
                        <div class=format!(
                            "flex flex-col gap-1.5 max-w-[80%] {}",
                            if is_user {
                                "items-end"
                            } else if is_system {
                                "items-center max-w-full"
                            } else {
                                "items-start"
                            }
                        )>
                            {if is_system {
                                None
                            } else {
                                Some(view! {
                                    <span class="text-2xs uppercase tracking-wider font-medium text-muted-foreground px-1">
                                        {role_label}
                                    </span>
                                })
                            }}
                            <div class=format!(
                                "rounded-lg {} {} {} {} chat-message-bubble {}",
                                if is_system && compact_layout.try_get().unwrap_or(false) {
                                    "px-0 py-0"
                                } else {
                                    "px-4 py-3"
                                },
                                if is_user {
                                    "bg-primary text-primary-foreground shadow-sm"
                                } else if is_system {
                                    "bg-transparent border-0 text-muted-foreground text-xs"
                                } else {
                                    "bg-muted/50 border border-border chat-message--assistant"
                                },
                                // Add min-height during streaming to prevent layout jump
                                if is_streaming { "min-h-[2.5rem]" } else { "" },
                                if is_system { "chat-message-system" } else { "" },
                                trust_state
                            )>
                                {if is_user {
                                    view! {
                                        <p class="text-sm whitespace-pre-wrap break-words leading-relaxed">
                                            {msg.content.clone()}
                                        </p>
                                    }.into_any()
                                } else if is_streaming {
                                    let has_content = !streaming_content.try_get().unwrap_or_default().is_empty();
                                    view! {
                                        <div class="text-sm break-words leading-relaxed">
                                            <MarkdownStream
                                                content=Signal::derive(move || streaming_content.try_get().unwrap_or_default())
                                            />
                                            {if has_content {
                                                view! {
                                                    <span class="inline-block animate-pulse text-primary/70 ml-0.5">"▍"</span>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <span class="inline-flex items-center gap-1.5 text-muted-foreground">
                                                        <Spinner/>
                                                        <span class="text-xs">"Preparing response..."</span>
                                                    </span>
                                                }.into_any()
                                            }}
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="text-sm break-words leading-relaxed">
                                            <Markdown content=msg.content.clone() />
                                        </div>
                                    }.into_any()
                                }}
                            </div>
                            // Run/Receipt links for assistant messages (placeholder if trace unavailable)
                            {if !is_user && !is_system && !is_streaming {
                                let latency = latency_ms.unwrap_or(0);
                                let trace = trace_id.clone();
                                let run_overview_url = trace.clone().map(|tid| format!("/runs/{}", tid));
                                let run_receipt_url = trace.clone().map(|tid| format!("/runs/{}?tab=receipt", tid));
                                let run_replay_url = trace.clone().map(|tid| format!("/runs/{}?tab=replay", tid));
                                Some(view! {
                                    {if !critical_notices.is_empty() {
                                        let title = prominent_degraded_title(&critical_notices).to_string();
                                        let alert_messages: Vec<String> = critical_notices
                                            .iter()
                                            .map(|notice| notice.message.clone())
                                            .collect::<BTreeSet<_>>()
                                            .into_iter()
                                            .collect();
                                        Some(view! {
                                            <div
                                                class="w-full rounded-lg border-2 border-destructive/60 bg-destructive/10 px-3 py-2 mb-1"
                                                data-testid="chat-meaning-change-alert"
                                            >
                                                <p class="text-xs font-semibold uppercase tracking-wide text-destructive">
                                                    {title}
                                                </p>
                                                <div class="mt-1 space-y-0.5">
                                                    {alert_messages.into_iter().map(|message| {
                                                        view! {
                                                            <p class="text-xs text-foreground leading-relaxed">
                                                                {message}
                                                            </p>
                                                        }
                                                    }).collect::<Vec<_>>()}
                                                </div>
                                            </div>
                                        })
                                    } else {
                                        None
                                    }}
                                    <div class="flex items-center gap-3 mt-1 px-1 flex-wrap" data-testid="chat-trace-links">
                                        {trace.clone().map(|tid| view! {
                                            <TraceButton
                                                trace_id=tid.clone()
                                                latency_ms=latency
                                                on_click=Callback::new(move |id: String| {
                                                    active_trace.set(Some(id));
                                                })
                                                data_testid="chat-trace-link".to_string()
                                            />
                                        })}
                                        <div class="flex items-center gap-1">
                                            {run_overview_url.map(|url| view! {
                                                <a
                                                    href=url
                                                    class="text-xs text-muted-foreground hover:text-primary transition-colors px-1.5 py-0.5 rounded hover:bg-muted"
                                                    title="View Execution Record"
                                                    data-testid="chat-run-link"
                                                >
                                                    "Execution Record"
                                                </a>
                                            }.into_any()).unwrap_or_else(|| view! {
                                                <span
                                                    class="text-xs text-muted-foreground/60 px-1.5 py-0.5 rounded"
                                                    title="Execution record unavailable"
                                                    data-testid="chat-run-link"
                                                >
                                                    "Execution Record"
                                                </span>
                                            }.into_any())}
                                            <span class="text-muted-foreground/50">"·"</span>
                                            {run_receipt_url.map(|url| view! {
                                                <a
                                                    href=url
                                                    class="text-xs text-muted-foreground hover:text-primary transition-colors px-1.5 py-0.5 rounded hover:bg-muted"
                                                    title="View Execution Receipt (signed log / proof)"
                                                    data-testid="chat-receipt-link"
                                                >
                                                    "Execution Receipt"
                                                </a>
                                            }.into_any()).unwrap_or_else(|| view! {
                                                <span
                                                    class="text-xs text-muted-foreground/60 px-1.5 py-0.5 rounded"
                                                    title="Execution receipt unavailable (signed log / proof unavailable)"
                                                    data-testid="chat-receipt-link"
                                                >
                                                    "Execution Receipt"
                                                </span>
                                            }.into_any())}
                                            <span class="text-muted-foreground/50">"·"</span>
                                            {run_replay_url.clone().map(|url| view! {
                                                <a
                                                    href=url
                                                    class="text-xs text-muted-foreground hover:text-primary transition-colors px-1.5 py-0.5 rounded hover:bg-muted"
                                                    title="Replay this response exactly"
                                                    data-testid="chat-replay-link"
                                                >
                                                    "Replay Exactly"
                                                </a>
                                            }.into_any()).unwrap_or_else(|| view! {
                                                <span
                                                    class="text-xs text-muted-foreground/60 px-1.5 py-0.5 rounded"
                                                    title="Replay unavailable"
                                                    data-testid="chat-replay-link"
                                                >
                                                    "Replay Exactly"
                                                </span>
                                            }.into_any())}
                                        </div>
                                        {token_count.map(|tc| {
                                            let display = format_token_display(tc, prompt_tokens, completion_tokens);
                                            view! {
                                                <span class="text-xs text-muted-foreground">
                                                    {display}
                                                </span>
                                            }
                                        })}
                                        {if has_trust_details {
                                            let summary = trust_summary_label(
                                                citation_count,
                                                document_link_count,
                                                &adapter_attachments,
                                                &adapters_used,
                                                degraded_notices.len(),
                                            );
                                            Some(view! {
                                                <div class="flex items-start gap-1.5 w-full flex-col" data-testid="chat-adapter-chips">
                                                    <span class="text-2xs text-muted-foreground" data-testid="chat-citation-chips">
                                                        {summary}
                                                    </span>
                                                    <ChatTrustPanel
                                                        citations=citations
                                                        document_links=document_links
                                                        adapters_used=adapters_used
                                                        adapter_attachments=adapter_attachments
                                                        degraded_notices=degraded_notices
                                                        unavailable_pinned_adapters=unavailable_pinned_adapters
                                                        pinned_routing_fallback=pinned_routing_fallback
                                                        fallback_triggered=fallback_triggered
                                                        fallback_backend=fallback_backend
                                                    />
                                                </div>
                                            })
                                        } else {
                                            None
                                        }}
                                    </div>
                                })
                            } else {
                                None
                            }}
                        </div>
                    </div>
                }
            })
        }}
    }
}

#[component]
fn ChatTrustPanel(
    citations: Vec<crate::signals::chat::ChatCitation>,
    document_links: Vec<crate::signals::chat::ChatDocumentLink>,
    adapters_used: Vec<String>,
    adapter_attachments: Vec<AdapterAttachment>,
    degraded_notices: Vec<DegradedNotice>,
    unavailable_pinned_adapters: Vec<String>,
    pinned_routing_fallback: Option<String>,
    fallback_triggered: bool,
    fallback_backend: Option<String>,
) -> impl IntoView {
    let dataset_versions: Vec<String> = document_links
        .iter()
        .filter_map(|link| link.dataset_version_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let mut effective_notices = degraded_notices;

    if !unavailable_pinned_adapters.is_empty()
        && !effective_notices
            .iter()
            .any(|notice| notice.kind == DegradedNoticeKind::BlockedPins)
    {
        effective_notices.push(DegradedNotice {
            kind: DegradedNoticeKind::BlockedPins,
            level: DegradedNoticeLevel::Warning,
            message: format!(
                "{} pinned adapter(s) were unavailable.",
                unavailable_pinned_adapters.len()
            ),
            meaning_changed: true,
        });
    }

    if let Some(mode) = pinned_routing_fallback.clone() {
        if !effective_notices
            .iter()
            .any(|notice| notice.kind == DegradedNoticeKind::RoutingOverride)
        {
            effective_notices.push(DegradedNotice {
                kind: DegradedNoticeKind::RoutingOverride,
                level: DegradedNoticeLevel::Warning,
                message: format!("Routing override applied with mode: {mode}."),
                meaning_changed: true,
            });
        }
    }

    if fallback_triggered
        && !effective_notices
            .iter()
            .any(|notice| notice.kind == DegradedNoticeKind::WorkerSemanticFallback)
    {
        let backend_label = fallback_backend
            .clone()
            .unwrap_or_else(|| "another backend".to_string());
        effective_notices.push(DegradedNotice {
            kind: DegradedNoticeKind::WorkerSemanticFallback,
            level: DegradedNoticeLevel::Critical,
            message: format!("Worker fallback changed execution backend to {backend_label}."),
            meaning_changed: true,
        });
    }

    view! {
        <details class="w-full rounded-md border border-border/60 bg-muted/20 px-2.5 py-2 mt-0.5" data-testid="chat-trust-panel">
            <summary class="cursor-pointer select-none text-2xs font-medium text-muted-foreground">
                "Trust details"
            </summary>
            <div class="mt-2 space-y-3">
                {if !adapter_attachments.is_empty() || !adapters_used.is_empty() {
                    Some(view! {
                        <div class="space-y-1" data-testid="chat-trust-adapters">
                            <p class="text-2xs uppercase tracking-wide text-muted-foreground">
                                "Why adapters were used"
                            </p>
                            {if !adapter_attachments.is_empty() {
                                adapter_attachments.into_iter().map(|attachment| {
                                    let display_name = attachment
                                        .adapter_label
                                        .clone()
                                        .unwrap_or_else(|| short_adapter_label(&attachment.adapter_id));
                                    let reason_label = attach_reason_label(&attachment.attach_reason);
                                    let reason_detail = attach_reason_detail(&attachment.attach_reason);
                                    view! {
                                        <div class="rounded-md border border-border/50 bg-background/70 px-2 py-1.5">
                                            <div class="flex items-center justify-between gap-2 flex-wrap">
                                                <span class="text-xs font-medium text-foreground">{display_name}</span>
                                                <span class="text-2xs uppercase tracking-wide text-muted-foreground">
                                                    {reason_label}
                                                </span>
                                            </div>
                                            <p class="text-2xs text-muted-foreground leading-relaxed">
                                                {reason_detail}
                                            </p>
                                            <p class="text-[11px] text-muted-foreground mt-1">
                                                "Version: "
                                                <span class="font-mono">
                                                    {attachment
                                                        .adapter_version_id
                                                        .clone()
                                                        .unwrap_or_else(|| "not pinned".to_string())}
                                                </span>
                                            </p>
                                            <p class="text-[10px] text-muted-foreground/80 mt-0.5">
                                                "ID: "
                                                <span class="font-mono">{attachment.adapter_id.clone()}</span>
                                            </p>
                                        </div>
                                    }
                                    .into_any()
                                }).collect::<Vec<_>>()
                            } else {
                                adapters_used.into_iter().map(|adapter_id| {
                                    view! {
                                        <div class="rounded-md border border-border/50 bg-background/70 px-2 py-1.5">
                                            <div class="flex items-center justify-between gap-2 flex-wrap">
                                                <span class="text-xs font-medium text-foreground">
                                                    {short_adapter_label(&adapter_id)}
                                                </span>
                                                <span class="text-2xs uppercase tracking-wide text-muted-foreground">
                                                    "used"
                                                </span>
                                            </div>
                                            <p class="text-[10px] text-muted-foreground/80 mt-0.5">
                                                "ID: "
                                                <span class="font-mono">{adapter_id}</span>
                                            </p>
                                        </div>
                                    }
                                    .into_any()
                                }).collect::<Vec<_>>()
                            }}
                        </div>
                    })
                } else {
                    None
                }}

                {if !effective_notices.is_empty() {
                    Some(view! {
                        <div class="space-y-1" data-testid="chat-trust-degraded">
                            <p class="text-2xs uppercase tracking-wide text-muted-foreground">
                                "Degraded or failed states"
                            </p>
                            {effective_notices.into_iter().map(|notice| {
                                let level_class = degraded_level_class(&notice.level);
                                view! {
                                    <div class=format!(
                                        "rounded-md border px-2 py-1.5 {}",
                                        level_class
                                    )>
                                        <div class="flex items-center justify-between gap-2 flex-wrap">
                                            <span class="text-xs font-medium text-foreground">
                                                {degraded_kind_label(&notice.kind)}
                                            </span>
                                            <span class="text-2xs uppercase tracking-wide text-muted-foreground">
                                                {degraded_level_label(&notice.level)}
                                            </span>
                                        </div>
                                        <p class="text-2xs text-muted-foreground leading-relaxed">
                                            {notice.message}
                                        </p>
                                        {if notice.meaning_changed {
                                            Some(view! {
                                                <p class="text-[11px] text-warning-foreground mt-1">
                                                    "Meaning changed from the requested path."
                                                </p>
                                            })
                                        } else {
                                            None
                                        }}
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    })
                } else {
                    None
                }}

                {if !document_links.is_empty() {
                    Some(view! {
                        <div class="space-y-1" data-testid="chat-trust-documents">
                            <p class="text-2xs uppercase tracking-wide text-muted-foreground">
                                "Source documents"
                            </p>
                            {if !dataset_versions.is_empty() {
                                Some(view! {
                                    <div class="flex flex-wrap items-center gap-1">
                                        {dataset_versions.into_iter().map(|dataset_version| {
                                            view! {
                                                <span class="text-2xs rounded bg-muted px-1.5 py-0.5 text-muted-foreground">
                                                    "Dataset version "
                                                    <span class="font-mono">{dataset_version}</span>
                                                </span>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                })
                            } else {
                                None
                            }}
                            <div class="flex flex-col gap-1" data-testid="chat-document-links">
                                {document_links.into_iter().map(|link| {
                                    let document_name = link.document_name.clone();
                                    let download_url = link.download_url.clone();
                                    let dataset_version = link.dataset_version_id.clone();
                                    let source_file = link.source_file.clone();
                                    view! {
                                        <div class="rounded-md border border-border/50 bg-background/70 px-2 py-1.5">
                                            <a
                                                href=download_url
                                                target="_blank"
                                                rel="noopener noreferrer"
                                                class="text-xs text-primary hover:underline"
                                                title="Open source document"
                                            >
                                                {document_name}
                                            </a>
                                            <div class="mt-1 flex flex-wrap gap-x-3 gap-y-0.5">
                                                {dataset_version.map(|dataset_version| view! {
                                                    <span class="text-[11px] text-muted-foreground">
                                                        "Dataset version: "
                                                        <span class="font-mono">{dataset_version}</span>
                                                    </span>
                                                })}
                                                {source_file.map(|source_file| view! {
                                                    <span class="text-[11px] text-muted-foreground">
                                                        "Source file: "
                                                        <span>{source_file}</span>
                                                    </span>
                                                })}
                                            </div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    })
                } else {
                    None
                }}

                {if !citations.is_empty() {
                    Some(view! {
                        <div class="space-y-1" data-testid="chat-trust-citations">
                            <p class="text-2xs uppercase tracking-wide text-muted-foreground">
                                "Citation spans"
                            </p>
                            <div class="flex flex-col gap-1">
                                {citations.into_iter().map(|citation| {
                                    view! {
                                        <div class="rounded-md border border-border/50 bg-background/70 px-2 py-1.5">
                                            <div class="flex items-center justify-between gap-2 flex-wrap">
                                                <span class="text-xs text-foreground">
                                                    {citation_page_span_label(&citation)}
                                                </span>
                                                {citation.rank.map(|rank| view! {
                                                    <span class="text-2xs text-muted-foreground">
                                                        "Rank "
                                                        {rank}
                                                    </span>
                                                })}
                                            </div>
                                            <p class="text-[11px] text-muted-foreground mt-0.5 truncate">
                                                {citation.file_path}
                                            </p>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    })
                } else {
                    None
                }}
            </div>
        </details>
    }
}

/// Chat conversation panel - renders the full conversation experience for a session.
/// Used by both /chat/history and /chat/s/:session_id routes through the ChatWorkspace layout.
#[component]
pub(super) fn ChatConversationPanel(
    /// Reactive session ID signal
    session_id_signal: Signal<String>,
    /// Monotonic epoch for local session index changes.
    session_index_epoch: Signal<u64>,
    /// Whether to process ?prompt= and ?adapter= query parameters
    #[prop(default = false)]
    handle_query_params: bool,
    /// Callback to refresh the session list sidebar.
    refresh_sessions: Callback<()>,
) -> impl IntoView {
    let session_id = move || session_id_signal.try_get().unwrap_or_default();
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
    let settings = use_settings();
    let (system_status, _refetch_status) = use_system_status();
    let status_center = use_status_center();
    let is_compact_view = use_is_tablet_or_smaller();
    let show_mobile_config_details = RwSignal::new(false);

    // Local state for input and trace panel
    let message = RwSignal::new(String::new());
    let active_trace = RwSignal::new(Option::<String>::None);
    let session_loaded = RwSignal::new(false);
    let current_session_id = RwSignal::new(String::new());
    let session_confirmation_state = RwSignal::new(SessionConfirmationState::Confirmed);
    let session_inline_notice = RwSignal::new(Option::<String>::None);
    let session_confirmation_nonce = RwSignal::new(0_u64);
    let session_confirmation_retry_epoch = RwSignal::new(0_u64);
    let session_confirmation_attempt = RwSignal::new(0_u64);
    // Guard so deep-link query params are processed once per session ID.
    // This fixes in-app navigations like `/chat/s/<newid>?adapter=...` while already on a chat route.
    let query_params_consumed_for_session = RwSignal::new(Option::<String>::None);

    // Auto-prune untouched placeholder sessions if the conversation panel unmounts.
    {
        on_cleanup(move || {
            let id = current_session_id.try_get_untracked().unwrap_or_default();
            if !id.is_empty() {
                ChatSessionsManager::prune_placeholder_session(&id);
                refresh_sessions.run(());
            }
        });
    }
    let verified_mode =
        Signal::derive(move || chat_state.try_get().unwrap_or_default().verified_mode);
    let bit_identical_mode_blocked = Signal::derive(move || {
        chat_state
            .try_get()
            .unwrap_or_default()
            .bit_identical_mode_blocked
    });
    let bit_identical_mode_degraded = Signal::derive(move || {
        chat_state
            .try_get()
            .unwrap_or_default()
            .bit_identical_mode_degraded
    });
    let show_attach_dialog = RwSignal::new(false);
    let attach_mode = RwSignal::new(AttachMode::Upload);
    let selected_file_name = RwSignal::new(Option::<String>::None);
    let selected_file = StoredValue::new_local(Option::<web_sys::File>::None);
    let attach_status = RwSignal::new(Option::<String>::None);
    let attach_error = RwSignal::new(Option::<String>::None);
    let attach_busy = RwSignal::new(false);
    let pasted_text = RwSignal::new(String::new());
    // Selected message indices for chat-to-dataset feature
    let selected_msg_indices = RwSignal::new(std::collections::HashSet::<usize>::new());
    // Cancellation signal to abort in-flight uploads when dialog is closed
    let upload_cancelled = RwSignal::new(false);
    #[cfg(target_arch = "wasm32")]
    let navigate = use_navigate();

    // Load session from localStorage when session ID or local session index changes.
    {
        let action = chat_action.clone();
        Effect::new(move |prev_effect_key: Option<(String, u64, u64)>| {
            let id = session_id();
            let observe_session_epoch = matches!(
                session_confirmation_state
                    .try_get()
                    .unwrap_or(SessionConfirmationState::Confirmed),
                SessionConfirmationState::PendingConfirm
                    | SessionConfirmationState::TransientError
                    | SessionConfirmationState::NotFound
            );
            let session_epoch = if observe_session_epoch {
                session_index_epoch.try_get().unwrap_or(0)
            } else {
                0
            };
            let retry_epoch = session_confirmation_retry_epoch.try_get().unwrap_or(0);
            let effect_key = (id.clone(), session_epoch, retry_epoch);

            // Handle empty/invalid session ID - redirect to landing page
            if id.is_empty() {
                web_sys::console::warn_1(
                    &"[ChatSession] Empty session ID, redirecting to /chat".into(),
                );
                if let Some(window) = web_sys::window() {
                    let _ = window.location().set_href("/chat");
                }
                return effect_key;
            }

            // Skip if both session and trigger epochs are unchanged.
            if prev_effect_key.as_ref() == Some(&effect_key) {
                return effect_key;
            }

            let session_changed = prev_effect_key
                .as_ref()
                .map(|(prev_id, _, _)| prev_id != &id)
                .unwrap_or(true);

            // Clear any existing messages from a different session before loading
            if let Some((ref prev, _, _)) = prev_effect_key {
                if !prev.is_empty() && prev != &id {
                    // Auto-prune untouched placeholder sessions when leaving.
                    ChatSessionsManager::prune_placeholder_session(prev);
                    refresh_sessions.run(());
                    action.clear_messages();
                }
            }

            // Validate session id before creating any placeholder state.
            if !ChatSessionsManager::is_valid_session_id(&id) {
                web_sys::console::warn_1(
                    &format!(
                        "[ChatSession] Invalid session ID '{}', redirecting to /chat",
                        id
                    )
                    .into(),
                );
                let navigate = use_navigate();
                navigate(
                    "/chat",
                    leptos_router::NavigateOptions {
                        replace: true,
                        ..Default::default()
                    },
                );
                return effect_key;
            }

            current_session_id.set(id.clone());
            action.set_session_id(Some(id.clone()));
            if session_changed {
                session_loaded.set(false);
                session_inline_notice.set(None);
            }

            // Try to load session from localStorage
            if let Some(stored) = ChatSessionsManager::load_session(&id) {
                let msg_count = stored.messages.len();
                let is_stub = msg_count == 0 && !stored.placeholder;
                action.restore_session(stored);
                session_confirmation_state.set(SessionConfirmationState::Confirmed);
                session_inline_notice.set(None);
                session_confirmation_nonce.update(|nonce| *nonce = nonce.wrapping_add(1));
                crate::debug_log!("[Chat] Restored session {} with {} messages", id, msg_count);
                // If this is a server-recovered stub with no local messages,
                // fetch messages from the backend and restore them.
                if is_stub {
                    let action = action.clone();
                    let id = id.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        match action.fetch_session_messages(&id).await {
                            Ok(messages) if !messages.is_empty() => {
                                if let Some(updated) =
                                    ChatSessionsManager::backfill_session_messages(&id, &messages)
                                {
                                    action.restore_session(updated);
                                    refresh_sessions.run(());
                                }
                            }
                            Ok(_) => {} // No messages on server either
                            Err(e) => {
                                web_sys::console::warn_1(
                                    &format!(
                                        "[Chat] Failed to backfill messages for {}: {}",
                                        id, e
                                    )
                                    .into(),
                                );
                            }
                        }
                    });
                }
            } else {
                // Session not found locally; create a local draft but do not persist it yet.
                // This keeps URL navigation stable without creating phantom sessions.
                let placeholder = ChatSessionsManager::create_placeholder_session(&id);
                action.restore_session(placeholder);
                session_confirmation_state.set(SessionConfirmationState::PendingConfirm);
                session_inline_notice.set(None);
                let nonce = session_confirmation_nonce
                    .try_get_untracked()
                    .unwrap_or(0)
                    .wrapping_add(1);
                session_confirmation_nonce.set(nonce);
                let attempt = session_confirmation_attempt
                    .try_get_untracked()
                    .unwrap_or(0)
                    .wrapping_add(1);
                session_confirmation_attempt.set(attempt);
                crate::debug_log!(
                    "[ChatSessionConfirm] state=pending session={} attempt={} source=local_miss",
                    id,
                    attempt
                );

                let action = action.clone();
                let id = id.clone();
                let current_session_id = current_session_id;
                let session_confirmation_nonce = session_confirmation_nonce;
                let session_confirmation_state = session_confirmation_state;
                let session_inline_notice = session_inline_notice;
                wasm_bindgen_futures::spawn_local(async move {
                    #[cfg(target_arch = "wasm32")]
                    gloo_timers::future::TimeoutFuture::new(1200).await;

                    let still_current = current_session_id.try_get_untracked().unwrap_or_default()
                        == id
                        && session_confirmation_nonce.try_get_untracked().unwrap_or(0) == nonce;
                    if !still_current {
                        return;
                    }

                    if let Some(stored_after_grace) = ChatSessionsManager::load_session(&id) {
                        action.restore_session(stored_after_grace);
                        session_confirmation_state.set(SessionConfirmationState::Confirmed);
                        session_inline_notice.set(None);
                        crate::debug_log!(
                            "[ChatSessionConfirm] state=confirmed session={} attempt={} source=local_after_grace",
                            id,
                            attempt
                        );
                        return;
                    }

                    match action.get_backend_session(&id).await {
                        Ok(backend_session) => {
                            let still_current =
                                current_session_id.try_get_untracked().unwrap_or_default() == id
                                    && session_confirmation_nonce.try_get_untracked().unwrap_or(0)
                                        == nonce;
                            if !still_current {
                                return;
                            }
                            let _ = ChatSessionsManager::merge_backend_sessions(
                                std::slice::from_ref(&backend_session),
                            );
                            refresh_sessions.run(());
                            if let Some(restored) = ChatSessionsManager::load_session(&id) {
                                action.restore_session(restored);
                            }
                            session_confirmation_state.set(SessionConfirmationState::Confirmed);
                            session_inline_notice.set(None);
                            crate::debug_log!(
                                "[ChatSessionConfirm] state=confirmed session={} attempt={} source=backend_probe",
                                id,
                                attempt
                            );
                        }
                        Err(e) => {
                            let still_current =
                                current_session_id.try_get_untracked().unwrap_or_default() == id
                                    && session_confirmation_nonce.try_get_untracked().unwrap_or(0)
                                        == nonce;
                            if !still_current {
                                return;
                            }
                            let mapped = map_session_confirmation_error(&e);
                            session_confirmation_state.set(mapped);
                            session_inline_notice.set(None);
                            let outcome = match mapped {
                                SessionConfirmationState::NotFound => "not_found",
                                SessionConfirmationState::TransientError => "transient",
                                SessionConfirmationState::Confirmed => "confirmed",
                                SessionConfirmationState::PendingConfirm => "pending",
                            };
                            crate::debug_log!(
                                "[ChatSessionConfirm] state={} session={} attempt={} error={}",
                                outcome,
                                id,
                                attempt,
                                e
                            );
                        }
                    }
                });
            }

            // Check for ?prompt=, ?adapter=, and ?add_files=1 query parameters once per session ID.
            if handle_query_params
                && query_params_consumed_for_session
                    .try_get_untracked()
                    .flatten()
                    .as_deref()
                    != Some(&id)
            {
                let mut consumed_any = false;
                #[cfg(target_arch = "wasm32")]
                let mut adapter_for_url: Option<String> = None;
                if let Some(window) = web_sys::window() {
                    if let Ok(search) = window.location().search() {
                        if let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) {
                            // Handle ?adapter= parameter - auto-pin the adapter
                            if let Some(adapter_id) = params.get("adapter") {
                                let decoded_adapter = js_sys::decode_uri_component(&adapter_id)
                                    .map(|s| s.as_string().unwrap_or_default())
                                    .unwrap_or(adapter_id);
                                if !decoded_adapter.is_empty() {
                                    let adapter = decoded_adapter;
                                    // Session-only pin (does not persist to localStorage)
                                    action.set_session_pinned_adapters(vec![adapter.clone()]);
                                    // Also set one-shot selected adapter so the first send definitely uses it.
                                    let Some(state) = chat_state.try_get_untracked() else {
                                        return effect_key.clone();
                                    };
                                    if state.selected_adapter.as_deref() != Some(adapter.as_str()) {
                                        action.select_next_adapter(&adapter);
                                    }
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        adapter_for_url = Some(adapter);
                                    }
                                    consumed_any = true;
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
                                    session_inline_notice.set(Some(format!(
                                        "Prompt too long ({} characters). Maximum is {} characters.",
                                        decoded.len(),
                                        MAX_URL_PROMPT_LENGTH
                                    )));
                                    return effect_key;
                                }
                                if !decoded.is_empty() {
                                    action.send_message_streaming(decoded);
                                    consumed_any = true;
                                }
                            }

                            // Handle ?add_files=1 parameter
                            if let Some(add_files) = params.get("add_files") {
                                if add_files == "1" || add_files.eq_ignore_ascii_case("true") {
                                    show_attach_dialog.set(true);
                                    consumed_any = true;
                                }
                            }
                        }
                    }
                }
                if consumed_any {
                    query_params_consumed_for_session.set(Some(id.clone()));
                    // Drop one-shot params (`prompt`, `add_files`) from the URL to avoid accidental re-run
                    // on refresh/back-button. Keep ?adapter= so a reload can re-apply session-only pins.
                    #[cfg(target_arch = "wasm32")]
                    {
                        let navigate = use_navigate();
                        let mut path = format!("/chat/s/{}", id);
                        if let Some(adapter) = adapter_for_url {
                            let encoded = js_sys::encode_uri_component(&adapter)
                                .as_string()
                                .unwrap_or(adapter);
                            path = format!("{}?adapter={}", path, encoded);
                        }
                        navigate(
                            &path,
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                }
            }

            session_loaded.set(true);
            effect_key
        });
    }

    // Auto-save session when messages change
    // Uses chat_state.try_get().unwrap_or_default() to create reactive dependency, then compares with previous state
    {
        Effect::new(move |prev_state: Option<(usize, bool, bool)>| {
            // Get state reactively to trigger effect when it changes
            let state = chat_state.try_get().unwrap_or_default();
            let msg_count = state.messages.len();
            let is_streaming = state.streaming;
            let verified_mode = state.verified_mode;
            // Get session ID untracked since we only care about state changes, not ID changes
            let id = current_session_id.try_get_untracked().unwrap_or_default();

            // Only save if:
            // 1. We have a session ID and messages
            // 2. Not currently streaming (wait for stream to complete)
            // 3. Message count changed OR streaming just stopped
            let should_save = !id.is_empty() && msg_count > 0 && !is_streaming;

            if should_save {
                if let Some((prev_count, was_streaming, prev_verified)) = prev_state {
                    // Save when message count changes, streaming just completed, or mode toggled
                    if msg_count != prev_count
                        || (was_streaming && !is_streaming)
                        || verified_mode != prev_verified
                    {
                        session_confirmation_state.set(SessionConfirmationState::Confirmed);
                        session_inline_notice.set(None);
                        session_confirmation_nonce.update(|nonce| *nonce = nonce.wrapping_add(1));
                        let session = ChatSessionsManager::session_from_state(&id, &state);
                        ChatSessionsManager::save_session(&session);
                        refresh_sessions.run(());
                        crate::debug_log!(
                            "[Chat] Auto-saved session {} ({} messages)",
                            id,
                            msg_count
                        );
                    }
                }
            }

            (msg_count, is_streaming, verified_mode)
        });
    }

    // Reset attach dialog state when closed
    {
        Effect::new(move || {
            if !show_attach_dialog.try_get().unwrap_or(false) {
                // Signal cancellation to abort any in-flight uploads
                let _ = upload_cancelled.try_set(true);
                let _ = attach_mode.try_set(AttachMode::Upload);
                let _ = selected_file_name.try_set(None);
                selected_file.set_value(None);
                let _ = attach_status.try_set(None);
                let _ = attach_error.try_set(None);
                let _ = attach_busy.try_set(false);
                let _ = pasted_text.try_set(String::new());
                let _ = selected_msg_indices.try_set(std::collections::HashSet::new());
            }
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
        let state = chat_state.try_get().unwrap_or_default();
        (
            state.loading,
            state.streaming,
            state.error.clone(),
            state.stream_recovery.is_some(),
        )
    });

    let is_loading = Signal::derive(move || chat_snapshot.try_get().unwrap_or_default().0);
    let is_streaming = Signal::derive(move || chat_snapshot.try_get().unwrap_or_default().1);
    let is_busy = Signal::derive(move || {
        let (loading, streaming, _, _) = chat_snapshot.try_get().unwrap_or_default();
        loading || streaming
    });
    // NOTE(chat-session-confirmation): Send is intentionally ungated by confirmation state.
    // Strict server-confirmed gating can be added here if product decides to enforce it.
    // See SessionConfirmationState for the available states.
    let can_send = Memo::new(move |_| {
        !message.try_get().unwrap_or_default().trim().is_empty()
            && !is_busy.try_get().unwrap_or(false)
    });
    let error = Signal::derive(move || chat_snapshot.try_get().unwrap_or_default().2);
    let can_retry = Signal::derive(move || {
        let (loading, streaming, _, has_recovery) = chat_snapshot.try_get().unwrap_or_default();
        !loading && !streaming && has_recovery
    });
    let retry_disabled = Signal::derive(move || !can_retry.try_get().unwrap_or(false));
    // Extract the active model name from system status for resolving "Auto" targets.
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
    let base_model_label =
        Signal::derive(
            move || match chat_state.try_get().unwrap_or_default().target.clone() {
                ChatTarget::Model(name) => name,
                _ => active_model_name
                    .try_get()
                    .flatten()
                    .unwrap_or_else(|| "Auto".to_string()),
            },
        );
    let base_model_badge = Signal::derive(move || {
        format!(
            "Base model: {}",
            base_model_label.try_get().unwrap_or_default()
        )
    });
    let context_model_label = Signal::derive(move || {
        let model = base_model_label
            .try_get()
            .unwrap_or_else(|| "Auto".to_string());
        if model.chars().count() > 20 {
            format!("{}…", model.chars().take(20).collect::<String>())
        } else {
            model
        }
    });
    let context_adapter_label = Signal::derive(move || {
        let state = chat_state.try_get().unwrap_or_default();
        let mut pinned = state.pinned_adapters.clone();
        for id in &state.session_pinned_adapters {
            if !pinned.contains(id) {
                pinned.push(id.clone());
            }
        }

        let primary = if state.verified_mode {
            pinned.first().cloned()
        } else {
            state
                .selected_adapter
                .clone()
                .or_else(|| pinned.first().cloned())
        };
        let compact = primary.map(|value| {
            if value.chars().count() > 18 {
                format!("{}…", value.chars().take(18).collect::<String>())
            } else {
                value
            }
        });

        if state.verified_mode {
            match compact {
                Some(label) => format!("{label} (pinned)"),
                None => "No pinned adapter".to_string(),
            }
        } else {
            compact.unwrap_or_else(|| "Auto".to_string())
        }
    });
    let context_mode_label = Signal::derive(move || {
        let state = chat_state.try_get().unwrap_or_default();
        if !state.verified_mode {
            "Best-Effort".to_string()
        } else if state.bit_identical_mode_blocked || state.bit_identical_mode_degraded {
            "Strict-Replayable".to_string()
        } else {
            "Bit-Identical".to_string()
        }
    });
    let context_mode_variant = Signal::derive(move || {
        let state = chat_state.try_get().unwrap_or_default();
        if !state.verified_mode {
            BadgeVariant::Secondary
        } else if state.bit_identical_mode_blocked || state.bit_identical_mode_degraded {
            BadgeVariant::Warning
        } else {
            BadgeVariant::Success
        }
    });

    // Convert active_adapters to AdapterMagnets for the AdapterBar
    let adapter_magnets = Memo::new(move |_| {
        let state = chat_state.try_get().unwrap_or_default();
        let pinned = {
            let mut out = state.pinned_adapters.clone();
            for id in &state.session_pinned_adapters {
                if !out.contains(id) {
                    out.push(id.clone());
                }
            }
            out
        };
        state
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
                    is_pinned: pinned.contains(&info.adapter_id),
                }
            })
            .collect::<Vec<_>>()
    });

    // Pinned adapter IDs signal for ChatAdaptersRegion
    let pinned_adapters = Signal::derive(move || {
        let state = chat_state.try_get().unwrap_or_default();
        let mut out = state.pinned_adapters.clone();
        for id in &state.session_pinned_adapters {
            if !out.contains(id) {
                out.push(id.clone());
            }
        }
        out
    });

    // Adapter selection pending flag (set on pin toggle, cleared on SSE update)
    let adapter_selection_pending = Signal::derive(move || {
        chat_state
            .try_get()
            .unwrap_or_default()
            .adapter_selection_pending
    });

    // Convert suggested_adapters for the SuggestedAdaptersBar
    // Name/purpose are populated from topology; other fields remain optional
    let suggested_adapters = Memo::new(move |_| {
        let selected = chat_state.try_get().unwrap_or_default().selected_adapter;
        chat_state
            .try_get()
            .unwrap_or_default()
            .suggested_adapters
            .iter()
            .map(|s| SuggestedAdapterView {
                adapter_id: s.adapter_id.clone(),
                display_name: s.name.clone().unwrap_or_else(|| s.adapter_id.clone()),
                confidence: s.confidence,
                is_pinned: s.is_pinned,
                is_selected: selected.as_deref() == Some(&s.adapter_id),
                // Use adapter name as description if available
                disabled_reason: None,
                description: s.purpose.clone(),
                tags: None,
            })
            .collect::<Vec<_>>()
    });

    // Message log scroll management
    let message_log_ref = NodeRef::<leptos::html::Div>::new();
    let is_at_bottom = RwSignal::new(true);

    // Keyed message IDs for efficient message list updates.
    let message_ids = Memo::new(move |_| {
        chat_state
            .try_get()
            .unwrap_or_default()
            .messages
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>()
    });

    let latest_trace_id = Memo::new(move |_| {
        chat_state
            .try_get()
            .unwrap_or_default()
            .messages
            .iter()
            .rev()
            .find_map(|msg| msg.trace_id.clone())
    });

    let latest_run_url = Signal::derive(move || {
        latest_trace_id
            .try_get()
            .flatten()
            .map(|trace_id| format!("/runs/{}", trace_id))
    });

    let latest_replay_url = Signal::derive(move || {
        latest_trace_id
            .try_get()
            .flatten()
            .map(|trace_id| format!("/runs/{}?tab=replay", trace_id))
    });

    let latest_signed_log_url = Signal::derive(move || {
        latest_trace_id
            .try_get()
            .flatten()
            .map(|trace_id| format!("/runs/{}?tab=receipt", trace_id))
    });

    let active_drawer = RwSignal::new(Some(ChatDrawerKind::Evidence));
    let mobile_lane = RwSignal::new(ChatMobileLane::Conversation);
    let drawer_panel_ref = NodeRef::<leptos::html::Div>::new();

    {
        Effect::new(move |_| {
            if is_compact_view.try_get().unwrap_or(false) {
                active_drawer.set(None);
                return;
            }
            if active_drawer.try_get().flatten().is_some() {
                if let Some(panel) = drawer_panel_ref.get() {
                    panel.set_tab_index(0);
                    let _ = panel.focus();
                }
            }
        });
    }

    let chat_layout_vm = Memo::new(move |_| {
        let state = chat_state.try_get().unwrap_or_default();
        let has_error = state.error.is_some();
        let status_variant = if has_error {
            BadgeVariant::Destructive
        } else if state.loading {
            BadgeVariant::Warning
        } else if state.streaming {
            BadgeVariant::Success
        } else if state.paused_inference.is_some() {
            BadgeVariant::Warning
        } else {
            BadgeVariant::Secondary
        };
        let status_label = if has_error {
            "Error"
        } else if state.loading {
            "Connecting"
        } else if state.streaming {
            "Streaming"
        } else if state.paused_inference.is_some() {
            "Paused"
        } else {
            "Ready"
        };

        let model = match state.target.clone() {
            ChatTarget::Model(name) => name,
            _ => active_model_name
                .try_get()
                .flatten()
                .unwrap_or_else(|| "Auto".to_string()),
        };
        let context_model_label = if model.chars().count() > 20 {
            format!("{}…", model.chars().take(20).collect::<String>())
        } else {
            model
        };

        let mut pinned = state.pinned_adapters.clone();
        for id in &state.session_pinned_adapters {
            if !pinned.contains(id) {
                pinned.push(id.clone());
            }
        }
        let primary = if state.verified_mode {
            pinned.first().cloned()
        } else {
            state
                .selected_adapter
                .clone()
                .or_else(|| pinned.first().cloned())
        };
        let compact_adapter = primary.map(|value| {
            if value.chars().count() > 18 {
                format!("{}…", value.chars().take(18).collect::<String>())
            } else {
                value
            }
        });
        let context_adapter_label = if state.verified_mode {
            match compact_adapter {
                Some(label) => format!("{label} (pinned)"),
                None => "No pinned adapter".to_string(),
            }
        } else {
            compact_adapter.unwrap_or_else(|| "Auto".to_string())
        };

        let mode_label = if !state.verified_mode {
            "Best-Effort".to_string()
        } else if state.bit_identical_mode_blocked || state.bit_identical_mode_degraded {
            "Strict-Replayable".to_string()
        } else {
            "Bit-Identical".to_string()
        };
        let mode_variant = if !state.verified_mode {
            BadgeVariant::Secondary
        } else if state.bit_identical_mode_blocked || state.bit_identical_mode_degraded {
            BadgeVariant::Warning
        } else {
            BadgeVariant::Success
        };

        let latest_assistant = state.messages.iter().rev().find(|msg| msg.role == "assistant");
        let citations = latest_assistant
            .and_then(|msg| msg.citations.clone())
            .unwrap_or_default();
        let doc_links = latest_assistant
            .and_then(|msg| msg.document_links.clone())
            .unwrap_or_default();
        let adapter_attachments = latest_assistant
            .map(|msg| msg.adapter_attachments.clone())
            .unwrap_or_default();
        let adapters_used = latest_assistant
            .and_then(|msg| msg.adapters_used.clone())
            .unwrap_or_default();
        let degraded_notices = latest_assistant
            .map(|msg| msg.degraded_notices.clone())
            .unwrap_or_default();
        let degraded_count = degraded_notices.len();
        let meaning_change_count = degraded_notices
            .iter()
            .filter(|notice| notice.meaning_changed)
            .count();

        ChatSessionLayoutVm {
            status_label: status_label.to_string(),
            status_variant,
            context_model_label,
            context_adapter_label,
            mode_label,
            mode_variant,
            trust_summary: trust_summary_label(
                citations.len(),
                doc_links.len(),
                &adapter_attachments,
                &adapters_used,
                degraded_count,
            ),
            latest_run_url: latest_run_url.try_get().flatten(),
            latest_replay_url: latest_replay_url.try_get().flatten(),
            latest_signed_log_url: latest_signed_log_url.try_get().flatten(),
            degraded_count,
            meaning_change_count,
        }
    });

    // Track tail updates so auto-scroll follows streaming token appends.
    let message_tail_signature = Memo::new(move |_| {
        let state = chat_state.try_get().unwrap_or_default();
        let tail = state
            .messages
            .last()
            .map(|m| (m.id.clone(), m.content.len(), m.is_streaming));
        (state.messages.len(), tail)
    });

    let scroll_to_latest = Callback::new(move |_: ()| {
        if let Some(el) = message_log_ref.get() {
            el.set_scroll_top(el.scroll_height());
            let _ = is_at_bottom.try_set(true);
        }
    });

    {
        Effect::new(move |_| {
            let _ = message_tail_signature.try_get();
            if !is_at_bottom.try_get().unwrap_or(true) {
                return;
            }
            if let Some(el) = message_log_ref.get() {
                let el_clone = el.clone();
                gloo_timers::callback::Timeout::new(10, move || {
                    el_clone.set_scroll_top(el_clone.scroll_height());
                    let _ = is_at_bottom.try_set(true);
                })
                .forget();
            }
        });
    }

    // Debounce version counter for preview calls
    let preview_version = RwSignal::new(0u64);

    // Debounced effect to preview adapters when input changes
    {
        let action = chat_action.clone();
        Effect::new(move |_| {
            let text = message.try_get().unwrap_or_default();
            // Update version to invalidate pending previews
            preview_version.update(|v| *v += 1);
            let current_version = preview_version.try_get_untracked().unwrap_or(0);

            // Debounce: 300ms delay before calling preview
            let action = action.clone();
            set_timeout_simple(
                move || {
                    // Only proceed if this is still the latest version
                    // (bail if signal is disposed — component was unmounted)
                    let Some(v) = preview_version.try_get_untracked() else {
                        return;
                    };
                    if v != current_version {
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

    // Select adapter for next message (one-shot override)
    let on_select_override = {
        let action = chat_action.clone();
        Callback::new(move |adapter_id: String| {
            action.select_next_adapter(&adapter_id);
        })
    };

    // Set full pinned adapter list (from manage dialog)
    let on_set_pinned = {
        let action = chat_action.clone();
        Callback::new(move |adapter_ids: Vec<String>| {
            action.set_pinned_adapters(adapter_ids);
        })
    };

    // Send message handler
    let do_send = {
        let action = chat_action.clone();
        Callback::new(move |_: ()| {
            let msg = message.try_get().unwrap_or_default();
            if !msg.trim().is_empty() {
                message.set(String::new());
                action.send_message_streaming(msg);
            }
        })
    };
    let retry_session_confirmation = {
        Callback::new(move |_: ()| {
            let id = current_session_id.try_get_untracked().unwrap_or_default();
            if id.is_empty() {
                return;
            }
            session_confirmation_state.set(SessionConfirmationState::PendingConfirm);
            session_inline_notice.set(None);
            session_confirmation_retry_epoch.update(|epoch| *epoch = epoch.wrapping_add(1));
            crate::debug_log!(
                "[ChatSessionConfirm] state=pending session={} source=manual_retry",
                id
            );
        })
    };

    // Keep persistent knowledge collection from user settings in chat state.
    {
        let action = chat_action.clone();
        Effect::new(move || {
            let knowledge = settings
                .try_get()
                .and_then(|s| s.knowledge_collection_id.clone());
            action.set_knowledge_collection_id(knowledge);
        });
    }

    // Keyboard handler for Enter-to-send (without Shift for newlines)
    let handle_keydown = {
        let do_send = do_send.clone();
        Callback::new(move |ev: web_sys::KeyboardEvent| {
            // Enter without Shift submits; Enter with Shift allows newline
            if ev.key() == "Enter" && !ev.shift_key() && can_send.try_get().unwrap_or(false) {
                ev.prevent_default();
                do_send.run(());
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

    let clear_error = {
        let action = chat_action.clone();
        Callback::new(move |_: ()| {
            action.clear_error();
        })
    };

    let message_callbacks = build_message_callbacks(scroll_to_latest);
    let drawer_callbacks = build_drawer_callbacks(active_drawer, mobile_lane);
    let session_notice_callbacks =
        build_session_notice_callbacks(retry_session_confirmation, clear_error);

    // Attach data -> dataset draft
    let create_draft = {
        #[cfg(target_arch = "wasm32")]
        let navigate = navigate.clone();
        #[cfg(target_arch = "wasm32")]
        let chat_action = chat_action.clone();
        Callback::new(move |_: ()| {
            attach_error.set(None);
            let mode = attach_mode.try_get().unwrap_or(AttachMode::Upload);
            #[cfg(target_arch = "wasm32")]
            let knowledge_collection_id = chat_state
                .try_get()
                .and_then(|s| s.knowledge_collection_id.clone());
            #[cfg(target_arch = "wasm32")]
            let base_model_param = {
                let base_model_id = match chat_state.try_get().unwrap_or_default().target.clone() {
                    ChatTarget::Model(name) => Some(name),
                    _ => None,
                };
                base_model_id
                    .as_ref()
                    .map(|val| {
                        let encoded = js_sys::encode_uri_component(val)
                            .as_string()
                            .unwrap_or_else(|| val.clone());
                        format!("&base_model_id={}", encoded)
                    })
                    .unwrap_or_default()
            };

            match mode {
                AttachMode::Upload => {
                    let Some(file) = selected_file.get_value() else {
                        attach_error.set(Some("Select a file to upload.".to_string()));
                        return;
                    };
                    if let Err(validation_error) = validate_attach_upload_file(&file) {
                        attach_error.set(Some(validation_error));
                        selected_file_name.set(None);
                        selected_file.set_value(None);
                        return;
                    }

                    let file_name = file.name();
                    // Reset cancellation flag before starting
                    upload_cancelled.set(false);
                    attach_busy.set(true);
                    attach_status.set(Some(format!("Uploading {}...", file_name)));

                    #[cfg(target_arch = "wasm32")]
                    {
                        let chat_action = chat_action.clone();
                        let attach_status = attach_status;
                        let attach_error = attach_error;
                        let attach_busy = attach_busy;
                        let show_attach_dialog = show_attach_dialog;
                        let _base_model_param = base_model_param.clone();
                        let knowledge_collection_id = knowledge_collection_id.clone();

                        wasm_bindgen_futures::spawn_local(async move {
                            // Check cancellation before starting
                            if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                return;
                            }

                            let client = ApiClient::with_base_url(api_base_url());
                            match client.upload_document(&file).await {
                                Ok(doc) => {
                                    // Check cancellation before updating UI
                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }

                                    // Show "already indexed" notice if document was deduplicated
                                    let info_suffix = if doc.deduplicated {
                                        " (already indexed)"
                                    } else {
                                        ""
                                    };
                                    attach_status.set(Some(format!(
                                        "Processing document{}...",
                                        info_suffix
                                    )));
                                    let doc_id = doc.document_id.clone();
                                    let mut chunk_count = doc.chunk_count.unwrap_or(0) as usize;

                                    for _ in 0..60 {
                                        // Check cancellation before each poll
                                        if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                            return;
                                        }
                                        gloo_timers::future::TimeoutFuture::new(1000).await;
                                        // Check cancellation after sleep
                                        if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                            return;
                                        }
                                        match client.get_document(&doc_id).await {
                                            Ok(status) => match status.status.as_str() {
                                                "indexed" => {
                                                    // Check cancellation before navigation
                                                    if upload_cancelled
                                                        .try_get_untracked()
                                                        .unwrap_or(true)
                                                    {
                                                        return;
                                                    }
                                                    if let Some(count) = status.chunk_count {
                                                        chunk_count = count as usize;
                                                    }
                                                    attach_status.set(Some(
                                                        "Creating chat collection...".to_string(),
                                                    ));
                                                    let collection_name =
                                                        if knowledge_collection_id.is_some() {
                                                            format!("Chat: {} (merged)", file_name)
                                                        } else {
                                                            format!("Chat: {}", file_name)
                                                        };
                                                    match client
                                                        .create_collection(&crate::api::types::CreateCollectionRequest {
                                                            name: collection_name,
                                                            description: Some("Auto-created from chat attachment".to_string()),
                                                        })
                                                        .await
                                                    {
                                                        Ok(collection) => {
                                                            if let Some(knowledge_id) = &knowledge_collection_id {
                                                                attach_status.set(Some("Merging knowledge sources...".to_string()));
                                                                if let Ok(knowledge) = client.get_collection(knowledge_id).await {
                                                                    for source_doc in knowledge.documents {
                                                                        let _ = client
                                                                            .add_document_to_collection(
                                                                                &collection.collection_id,
                                                                                &source_doc.document_id,
                                                                            )
                                                                            .await;
                                                                    }
                                                                }
                                                            }
                                                            if let Err(e) = client
                                                                .add_document_to_collection(&collection.collection_id, &doc_id)
                                                                .await
                                                            {
                                                                attach_error.set(Some(format!(
                                                                    "Collection created but failed to attach document: {}",
                                                                    e
                                                                )));
                                                                attach_busy.set(false);
                                                                attach_status.set(None);
                                                                return;
                                                            }
                                                            chat_action.set_active_collection_id(Some(collection.collection_id.clone()));
                                                            let system_message = crate::signals::ChatMessage {
                                                                id: format!("sys-{}", uuid::Uuid::new_v4().simple()),
                                                                role: "system".to_string(),
                                                                content: format!(
                                                                    "📎 {} added ({} chunks). I can now answer questions about this document.",
                                                                    file_name, chunk_count
                                                                ),
                                                                timestamp: crate::utils::now_utc(),
                                                                is_streaming: false,
                                                                status: crate::signals::MessageStatus::Complete,
                                                                queued_at: None,
                                                                pending_phase: crate::signals::PendingPhase::Calm,
                                                                pending_reason: None,
                                                                trace_id: None,
                                                                latency_ms: None,
                                                                token_count: None,
                                                                prompt_tokens: None,
                                                                completion_tokens: None,
                                                                backend_used: None,
                                                                citations: None,
                                                                document_links: None,
                                                                has_citations: false,
                                                                adapters_used: None,
                                                                unavailable_pinned_adapters: None,
                                                                pinned_routing_fallback: None,
                                                                fallback_triggered: false,
                                                                fallback_backend: None,
                                                                adapter_attachments: Vec::new(),
                                                                degraded_notices: Vec::new(),
                                                                replay_status: None,
                                                                policy_warnings: Vec::new(),
                                                            };
                                                            chat_action.append_message(system_message);
                                                        }
                                                        Err(e) => {
                                                            attach_error.set(Some(format!(
                                                                "Failed to create collection for chat RAG: {}",
                                                                e
                                                            )));
                                                            attach_busy.set(false);
                                                            attach_status.set(None);
                                                            return;
                                                        }
                                                    }
                                                    show_attach_dialog.set(false);
                                                    attach_busy.set(false);
                                                    attach_status.set(None);
                                                    return;
                                                }
                                                "failed" => {
                                                    if upload_cancelled
                                                        .try_get_untracked()
                                                        .unwrap_or(true)
                                                    {
                                                        return;
                                                    }
                                                    attach_error.set(Some(format!(
                                                        "Document processing failed: {}",
                                                        status.error_message.unwrap_or_default()
                                                    )));
                                                    attach_busy.set(false);
                                                    attach_status.set(None);
                                                    return;
                                                }
                                                _ => {
                                                    if upload_cancelled
                                                        .try_get_untracked()
                                                        .unwrap_or(true)
                                                    {
                                                        return;
                                                    }
                                                    attach_status.set(Some(format!(
                                                        "Processing document ({})...",
                                                        status_display_with_raw(&status.status)
                                                    )));
                                                }
                                            },
                                            Err(e) => {
                                                if upload_cancelled
                                                    .try_get_untracked()
                                                    .unwrap_or(true)
                                                {
                                                    return;
                                                }
                                                attach_error.set(Some(format!(
                                                    "Failed to check status: {}",
                                                    e
                                                )));
                                                attach_busy.set(false);
                                                attach_status.set(None);
                                                return;
                                            }
                                        }
                                    }

                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }
                                    attach_error
                                        .set(Some("Document processing timed out.".to_string()));
                                    attach_busy.set(false);
                                    attach_status.set(None);
                                }
                                Err(e) => {
                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }
                                    attach_error.set(Some(format!("Upload failed: {}", e)));
                                    attach_busy.set(false);
                                    attach_status.set(None);
                                }
                            }
                        });
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let _ = file;
                        attach_error.set(Some(
                            "File upload is only available in the web UI.".to_string(),
                        ));
                        attach_busy.set(false);
                        attach_status.set(None);
                    }
                }
                AttachMode::Paste => {
                    let text = pasted_text.try_get().unwrap_or_default();
                    if text.trim().is_empty() {
                        attach_error.set(Some("Paste some text content first.".to_string()));
                        return;
                    }

                    // Reset cancellation flag before starting
                    upload_cancelled.set(false);
                    attach_busy.set(true);
                    attach_status.set(Some("Preparing your text...".to_string()));

                    #[cfg(target_arch = "wasm32")]
                    {
                        let navigate = navigate.clone();
                        let attach_status = attach_status;
                        let attach_error = attach_error;
                        let attach_busy = attach_busy;
                        let show_attach_dialog = show_attach_dialog;
                        let base_model_param = base_model_param.clone();

                        wasm_bindgen_futures::spawn_local(async move {
                            // Check cancellation before starting
                            if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                return;
                            }

                            let client = ApiClient::with_base_url(api_base_url());
                            match client
                                .create_dataset_from_text(
                                    text,
                                    Some("pasted-text".to_string()),
                                    None,
                                )
                                .await
                            {
                                Ok(resp) => {
                                    // Check cancellation before navigation
                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }
                                    let path = format!(
                                        "/training?open_wizard=1&dataset_id={}{}",
                                        resp.dataset_id, base_model_param
                                    );
                                    navigate(&path, Default::default());
                                    show_attach_dialog.set(false);
                                    attach_busy.set(false);
                                    attach_status.set(None);
                                }
                                Err(e) => {
                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }
                                    attach_error
                                        .set(Some(format!("Couldn't prepare your text: {}", e)));
                                    attach_busy.set(false);
                                    attach_status.set(None);
                                }
                            }
                        });
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        attach_error.set(Some(
                            "Text processing is only available in the web UI.".to_string(),
                        ));
                        attach_busy.set(false);
                        attach_status.set(None);
                    }
                }
                AttachMode::Chat => {
                    let indices = selected_msg_indices.try_get().unwrap_or_default();
                    if indices.is_empty() {
                        attach_error.set(Some("Select at least one message.".to_string()));
                        return;
                    }

                    let messages = chat_state.try_get().unwrap_or_default().messages;
                    let session_id = chat_state.try_get().unwrap_or_default().session_id.clone();

                    // Convert selected messages to ChatMessageInput format
                    let mut selected: Vec<(usize, ChatMessageInput)> = indices
                        .iter()
                        .filter_map(|&idx| {
                            messages.get(idx).map(|msg| {
                                (
                                    idx,
                                    ChatMessageInput {
                                        role: msg.role.clone(),
                                        content: msg.content.clone(),
                                        timestamp: Some(msg.timestamp.to_rfc3339()),
                                    },
                                )
                            })
                        })
                        .collect();
                    // Sort by index to preserve conversation order
                    selected.sort_by_key(|(idx, _)| *idx);
                    let chat_messages: Vec<ChatMessageInput> =
                        selected.into_iter().map(|(_, m)| m).collect();

                    // Reset cancellation flag before starting
                    upload_cancelled.set(false);
                    attach_busy.set(true);
                    attach_status.set(Some("Preparing selected messages...".to_string()));

                    #[cfg(target_arch = "wasm32")]
                    {
                        let navigate = navigate.clone();
                        let attach_status = attach_status;
                        let attach_error = attach_error;
                        let attach_busy = attach_busy;
                        let show_attach_dialog = show_attach_dialog;
                        let base_model_param = base_model_param.clone();

                        wasm_bindgen_futures::spawn_local(async move {
                            // Check cancellation before starting
                            if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                return;
                            }

                            let client = ApiClient::with_base_url(api_base_url());
                            match client
                                .create_dataset_from_chat(
                                    chat_messages,
                                    Some("chat-selection".to_string()),
                                    session_id,
                                )
                                .await
                            {
                                Ok(resp) => {
                                    // Check cancellation before navigation
                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }
                                    let path = format!(
                                        "/training?open_wizard=1&dataset_id={}{}",
                                        resp.dataset_id, base_model_param
                                    );
                                    navigate(&path, Default::default());
                                    show_attach_dialog.set(false);
                                    attach_busy.set(false);
                                    attach_status.set(None);
                                }
                                Err(e) => {
                                    if upload_cancelled.try_get_untracked().unwrap_or(true) {
                                        return;
                                    }
                                    attach_error.set(Some(format!(
                                        "Couldn't prepare selected messages: {}",
                                        e
                                    )));
                                    attach_busy.set(false);
                                    attach_status.set(None);
                                }
                            }
                        });
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let _ = (chat_messages, session_id);
                        attach_error.set(Some(
                            "Chat processing is only available in the web UI.".to_string(),
                        ));
                        attach_busy.set(false);
                        attach_status.set(None);
                    }
                }
            }
        })
    };

    let render_evidence_panel = move || {
        view! {
            <div class="chat-drawer-panel chat-drawer-panel--evidence" data-testid="chat-drawer-evidence">
                <div class="space-y-2">
                    <p class="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                        "Evidence lane"
                    </p>
                    <p class="text-sm text-foreground">
                        {move || {
                            chat_layout_vm
                                .try_get()
                                .map(|vm| vm.trust_summary.clone())
                                .unwrap_or_else(|| "Open trust details".to_string())
                        }}
                    </p>
                    <div class="flex flex-wrap items-center gap-2">
                        {move || {
                            chat_layout_vm.try_get().map(|vm| {
                                let degraded = vm.degraded_count;
                                let meaning_changed = vm.meaning_change_count;
                                view! {
                                    <>
                                        {if degraded > 0 {
                                            Some(view! {
                                                <span class="text-2xs rounded bg-warning/15 px-2 py-0.5 text-warning-foreground">
                                                    {format!("{} degraded notice{}", degraded, if degraded == 1 { "" } else { "s" })}
                                                </span>
                                            })
                                        } else {
                                            None
                                        }}
                                        {if meaning_changed > 0 {
                                            Some(view! {
                                                <span class="text-2xs rounded bg-destructive/15 px-2 py-0.5 text-destructive">
                                                    {format!("{} meaning change{}", meaning_changed, if meaning_changed == 1 { "" } else { "s" })}
                                                </span>
                                            })
                                        } else {
                                            None
                                        }}
                                    </>
                                }
                            })
                        }}
                    </div>
                    <div class="flex flex-wrap items-center gap-2 pt-1">
                        {move || {
                            let vm = chat_layout_vm.try_get().unwrap_or(ChatSessionLayoutVm {
                                status_label: "Ready".to_string(),
                                status_variant: BadgeVariant::Secondary,
                                context_model_label: "Auto".to_string(),
                                context_adapter_label: "Auto".to_string(),
                                mode_label: "Best-Effort".to_string(),
                                mode_variant: BadgeVariant::Secondary,
                                trust_summary: "Open trust details".to_string(),
                                latest_run_url: None,
                                latest_replay_url: None,
                                latest_signed_log_url: None,
                                degraded_count: 0,
                                meaning_change_count: 0,
                            });
                            view! {
                                <>
                                    {vm.latest_run_url.clone().map(|url| view! {
                                        <a
                                            href=url
                                            class="btn btn-ghost btn-sm"
                                            data-testid="chat-run-link"
                                        >
                                            "Execution Record"
                                        </a>
                                    }).unwrap_or_else(|| view! {
                                        <span class="text-xs text-muted-foreground/60 px-1.5 py-0.5 rounded" data-testid="chat-run-link">
                                            "Execution Record"
                                        </span>
                                    })}
                                    {vm.latest_signed_log_url.clone().map(|url| view! {
                                        <a
                                            href=url
                                            class="btn btn-ghost btn-sm"
                                            data-testid="chat-receipt-link"
                                        >
                                            "Execution Receipt"
                                        </a>
                                    }).unwrap_or_else(|| view! {
                                        <span class="text-xs text-muted-foreground/60 px-1.5 py-0.5 rounded" data-testid="chat-receipt-link">
                                            "Execution Receipt"
                                        </span>
                                    })}
                                    {vm.latest_replay_url.clone().map(|url| view! {
                                        <a
                                            href=url
                                            class="btn btn-ghost btn-sm"
                                            data-testid="chat-replay-link"
                                        >
                                            "Replay Exactly"
                                        </a>
                                    }).unwrap_or_else(|| view! {
                                        <span class="text-xs text-muted-foreground/60 px-1.5 py-0.5 rounded" data-testid="chat-replay-link">
                                            "Replay Exactly"
                                        </span>
                                    })}
                                </>
                            }
                        }}
                    </div>
                </div>

                {move || {
                    chat_layout_vm
                        .try_get()
                        .and_then(|vm| vm.latest_replay_url.clone())
                        .map(|replay_href| {
                            let signed_log_href = chat_layout_vm
                                .try_get()
                                .and_then(|vm| vm.latest_signed_log_url.clone())
                                .unwrap_or_else(|| "/runs".to_string());
                            view! {
                                <div
                                    class="mt-3 rounded-lg border border-primary/25 bg-primary/5 px-4 py-3"
                                    data-testid="chat-replay-proof-banner"
                                >
                                    <div class="flex flex-wrap items-center justify-between gap-3">
                                        <div class="space-y-1">
                                            <p class="text-sm font-medium">"Replay + execution receipt ready"</p>
                                            <p class="text-xs text-muted-foreground">
                                                "Replay the latest answer exactly and inspect the signed receipt."
                                            </p>
                                        </div>
                                        <div class="flex items-center gap-2">
                                            <ButtonLink href=replay_href variant=ButtonVariant::Primary size=ButtonSize::Sm>
                                                "Replay Exact Response"
                                            </ButtonLink>
                                            <ButtonLink href=signed_log_href variant=ButtonVariant::Outline size=ButtonSize::Sm>
                                                "View Execution Receipt"
                                            </ButtonLink>
                                        </div>
                                    </div>
                                </div>
                            }
                        })
                }}
            </div>
        }
    };

    let render_context_panel = move || {
        view! {
            <div class="chat-drawer-panel chat-drawer-panel--context" data-testid="chat-drawer-context">
                <div class="space-y-3">
                    <p class="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                        "Context lane"
                    </p>
                    <div class="chat-context-summary flex flex-wrap items-center gap-2 rounded-lg border border-border/60 bg-muted/20 px-3 py-2">
                        <Badge variant=BadgeVariant::Outline>
                            <span class="text-xs">
                                "Model: "
                                {move || {
                                    chat_layout_vm
                                        .try_get()
                                        .map(|vm| vm.context_model_label.clone())
                                        .unwrap_or_else(|| "Auto".to_string())
                                }}
                            </span>
                        </Badge>
                        <Badge variant=BadgeVariant::Outline>
                            <span class="text-xs">
                                "Adapter: "
                                {move || {
                                    chat_layout_vm
                                        .try_get()
                                        .map(|vm| vm.context_adapter_label.clone())
                                        .unwrap_or_else(|| "Auto".to_string())
                                }}
                            </span>
                        </Badge>
                        {move || {
                            let vm = chat_layout_vm.try_get().unwrap_or(ChatSessionLayoutVm {
                                status_label: "Ready".to_string(),
                                status_variant: BadgeVariant::Secondary,
                                context_model_label: "Auto".to_string(),
                                context_adapter_label: "Auto".to_string(),
                                mode_label: "Best-Effort".to_string(),
                                mode_variant: BadgeVariant::Secondary,
                                trust_summary: "Open trust details".to_string(),
                                latest_run_url: None,
                                latest_replay_url: None,
                                latest_signed_log_url: None,
                                degraded_count: 0,
                                meaning_change_count: 0,
                            });
                            view! {
                                <Badge variant=vm.mode_variant>
                                    <span class="text-xs">{vm.mode_label}</span>
                                </Badge>
                            }
                        }}
                    </div>
                    <div class="chat-context-target">
                        <ChatTargetSelector inline=true/>
                    </div>
                    <details class="rounded-lg border border-border/60 bg-card/60 px-3 py-2" data-testid="chat-advanced-adapter-controls">
                        <summary class="cursor-pointer text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                            "Advanced adapter controls"
                        </summary>
                        <div class="mt-3">
                            <ChatAdaptersRegion
                                active_adapters=adapter_magnets
                                pinned_adapters=pinned_adapters
                                suggestions=suggested_adapters
                                pending=adapter_selection_pending
                                on_select_override=on_select_override
                                on_toggle_pin=on_toggle_pin
                                on_set_pinned=on_set_pinned
                                loading=is_streaming
                            />
                        </div>
                    </details>
                </div>
            </div>
        }
    };

    view! {
        <div class="p-4 flex h-full min-h-0 flex-col gap-3">
            // Header
            <div
                class="flex flex-wrap items-start justify-between gap-3 border-b border-border pb-3"
                data-testid="chat-header"
            >
                <div class="flex items-center gap-2 text-xs text-muted-foreground">
                    <span class="uppercase tracking-wider text-2xs font-medium">"Session"</span>
                    <span
                        class="font-mono bg-muted/30 px-1.5 py-0.5 rounded text-2xs"
                        data-testid="chat-session-id-label"
                    >
                        {session_label}
                    </span>
                </div>
                <div class="chat-header-controls">
                    // Target selector for choosing model, stack, or policy pack
                    <div class="chat-header-target">
                        <ChatTargetSelector inline=true/>
                    </div>
                    <Badge variant=BadgeVariant::Outline class="chat-header-base-model">
                        <span
                            class="chat-header-base-model-label"
                            title=move || base_model_badge.try_get().unwrap_or_default()
                        >
                            {move || base_model_badge.try_get().unwrap_or_default()}
                        </span>
                    </Badge>
                    <div class="chat-header-mode-toggle flex items-center rounded-full border border-border bg-muted/30 p-0.5 text-xs">
                        {
                            let action = chat_action.clone();
                            view! {
                                <button
                                    class=move || {
                                        if verified_mode.try_get().unwrap_or(false) {
                                            "btn btn-ghost btn-sm px-2 py-1 rounded-full text-muted-foreground".to_string()
                                        } else {
                                            "btn btn-ghost btn-sm px-2 py-1 rounded-full bg-background text-foreground shadow-sm".to_string()
                                        }
                                    }
                                    on:click=move |_| action.set_verified_mode(false)
                                    type="button"
                                >
                                    "Best-Effort"
                                </button>
                            }
                        }
                        {
                            let action = chat_action.clone();
                            view! {
                                <button
                                    class=move || {
                                        if verified_mode.try_get().unwrap_or(false) {
                                            if bit_identical_mode_blocked.try_get().unwrap_or(false) {
                                                "btn btn-ghost btn-sm px-2 py-1 rounded-full bg-destructive/15 text-destructive shadow-sm".to_string()
                                            } else if bit_identical_mode_degraded.try_get().unwrap_or(false) {
                                                "btn btn-ghost btn-sm px-2 py-1 rounded-full bg-warning/15 text-warning-foreground shadow-sm".to_string()
                                            } else {
                                                "btn btn-ghost btn-sm px-2 py-1 rounded-full bg-background text-foreground shadow-sm".to_string()
                                            }
                                        } else {
                                            "btn btn-ghost btn-sm px-2 py-1 rounded-full text-muted-foreground".to_string()
                                        }
                                    }
                                    on:click=move |_| action.set_verified_mode(true)
                                    type="button"
                                >
                                    {move || {
                                        if bit_identical_mode_blocked.try_get().unwrap_or(false)
                                            || bit_identical_mode_degraded
                                                .try_get()
                                                .unwrap_or(false)
                                        {
                                            "Strict-Replayable"
                                        } else {
                                            "Bit-Identical"
                                        }
                                    }}
                                </button>
                            }
                        }
                    </div>
                    // Status badge
                    <div class="chat-header-status" data-testid="chat-status-badge">
                        {move || {
                            let vm = chat_layout_vm.try_get().unwrap_or(ChatSessionLayoutVm {
                                status_label: "Ready".to_string(),
                                status_variant: BadgeVariant::Secondary,
                                context_model_label: "Auto".to_string(),
                                context_adapter_label: "Auto".to_string(),
                                mode_label: "Best-Effort".to_string(),
                                mode_variant: BadgeVariant::Secondary,
                                trust_summary: "Open trust details".to_string(),
                                latest_run_url: None,
                                latest_replay_url: None,
                                latest_signed_log_url: None,
                                degraded_count: 0,
                                meaning_change_count: 0,
                            });
                            view! { <Badge variant=vm.status_variant>{vm.status_label}</Badge> }.into_any()
                        }}
                    </div>
                </div>
            </div>
            <div class="chat-session-layout flex-1 min-h-0">
                <div class="chat-main-column flex min-h-0 flex-col gap-3">
                    {move || {
                        if is_compact_view.try_get().unwrap_or(false) {
                            let lane = mobile_lane
                                .try_get()
                                .unwrap_or(ChatMobileLane::Conversation);
                            let button_classes = |active: bool| {
                                if active {
                                    "btn btn-sm btn-ghost rounded-md bg-background text-foreground shadow-sm"
                                } else {
                                    "btn btn-sm btn-ghost rounded-md text-muted-foreground"
                                }
                            };
                            Some(view! {
                                <div class="chat-lane-toggle flex items-center gap-1 rounded-lg border border-border/60 bg-muted/30 p-1">
                                    <button
                                        type="button"
                                        class=button_classes(matches!(lane, ChatMobileLane::Conversation))
                                        on:click=move |_| drawer_callbacks.set_lane_conversation.run(())
                                        data-testid="chat-lane-toggle-conversation"
                                    >
                                        "Conversation"
                                    </button>
                                    <button
                                        type="button"
                                        class=button_classes(matches!(lane, ChatMobileLane::Evidence))
                                        on:click=move |_| drawer_callbacks.set_lane_evidence.run(())
                                        data-testid="chat-lane-toggle-evidence"
                                    >
                                        "Evidence"
                                    </button>
                                    <button
                                        type="button"
                                        class=button_classes(matches!(lane, ChatMobileLane::Context))
                                        on:click=move |_| drawer_callbacks.set_lane_context.run(())
                                        data-testid="chat-lane-toggle-context"
                                    >
                                        "Context"
                                    </button>
                                </div>
                            })
                        } else {
                            None
                        }
                    }}
                    {move || {
                        let compact = is_compact_view.try_get().unwrap_or(false);
                        let lane = mobile_lane
                            .try_get()
                            .unwrap_or(ChatMobileLane::Conversation);
                        if !compact || matches!(lane, ChatMobileLane::Conversation) {
                            Some(view! {
            <div
                class="flex flex-wrap items-center gap-2 rounded-lg border border-border/60 bg-muted/20 px-3 py-2"
                data-testid="chat-context-strip"
            >
                <span class="text-[11px] uppercase tracking-wide text-muted-foreground">
                    "Current context"
                </span>
                <Badge variant=BadgeVariant::Outline>
                    <span class="text-xs">
                        "Model: "
                        {move || context_model_label.try_get().unwrap_or_else(|| "Auto".to_string())}
                    </span>
                </Badge>
                <Badge variant=BadgeVariant::Outline>
                    <span class="text-xs">
                        "Adapter: "
                        {move || context_adapter_label.try_get().unwrap_or_else(|| "Auto".to_string())}
                    </span>
                </Badge>
                {move || {
                    let variant = context_mode_variant
                        .try_get()
                        .unwrap_or(BadgeVariant::Secondary);
                    let label = context_mode_label
                        .try_get()
                        .unwrap_or_else(|| "Best-Effort".to_string());
                    view! {
                        <Badge variant=variant>
                            <span class="text-xs">{label}</span>
                        </Badge>
                    }
                }}
            </div>

            <ChatStreamAndPausedStatus chat_state=chat_state/>

            // Pending-adapter badge: rendered outside the collapsed <details>
            // so it's always visible when adapter_selection_pending is true.
            {move || adapter_selection_pending.try_get().unwrap_or(false).then(|| view! {
                <span
                    class="chat-adapters-pending-badge"
                    role="status"
                    aria-label="Adapter changes pending confirmation"
                >
                    "Pending next message"
                </span>
            })}

            <details class="rounded-lg border border-border/60 bg-card/60 px-3 py-2" data-testid="chat-advanced-adapter-controls">
                <summary class="cursor-pointer text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                    "Advanced adapter controls"
                </summary>
                <div class="mt-3">
                    // Unified Adapters Region: Active, Pinned, Suggested + Manage
                    <ChatAdaptersRegion
                        active_adapters=adapter_magnets
                        pinned_adapters=pinned_adapters
                        suggestions=suggested_adapters
                        pending=adapter_selection_pending
                        on_select_override=on_select_override
                        on_toggle_pin=on_toggle_pin
                        on_set_pinned=on_set_pinned
                        loading=is_streaming
                    />
                </div>
            </details>

            {move || {
                latest_replay_url.try_get().flatten().map(|replay_href| {
                    let signed_log_href = latest_signed_log_url
                        .try_get()
                        .flatten()
                        .unwrap_or_else(|| "/runs".to_string());
                    view! {
                        <div
                            class="rounded-lg border border-primary/25 bg-primary/5 px-4 py-3"
                            data-testid="chat-replay-proof-banner"
                        >
                            <div class="flex flex-wrap items-center justify-between gap-3">
                                <div class="space-y-1">
                                    <p class="text-sm font-medium">"Replay + execution receipt ready"</p>
                                    <p class="text-xs text-muted-foreground">
                                        "You can replay the latest response with locked output and review its execution receipt."
                                    </p>
                                </div>
                                <div class="flex items-center gap-2">
                                    <ButtonLink
                                        href=replay_href
                                        variant=ButtonVariant::Primary
                                        size=ButtonSize::Sm
                                    >
                                        "Replay Exact Response"
                                    </ButtonLink>
                                    <ButtonLink
                                        href=signed_log_href
                                        variant=ButtonVariant::Outline
                                        size=ButtonSize::Sm
                                    >
                                        "View Execution Receipt"
                                    </ButtonLink>
                                </div>
                            </div>
                        </div>
                    }
                })
            }}

            // Messages
            <div class="relative flex-1 min-h-0">
                <div
                    node_ref=message_log_ref
                    class="flex-1 h-full overflow-y-auto rounded-lg border border-border bg-card"
                    role="log"
                    aria-live="polite"
                    aria-label="Chat messages"
                    on:scroll=move |_| {
                        if let Some(el) = message_log_ref.get() {
                            let distance = el.scroll_height() - el.scroll_top() - el.client_height();
                            let _ = is_at_bottom.try_set(distance <= CHAT_SCROLL_BOTTOM_THRESHOLD_PX);
                        }
                    }
                >
                // Context overflow indicator
                {
                    let dismiss_action = chat_action.clone();
                    move || {
                        let notice = chat_state.try_get().unwrap_or_default().overflow_notice();
                        notice.map(|msg| {
                            let dismiss = dismiss_action.clone();
                            let evicted = chat_state.try_get().unwrap_or_default().total_messages_evicted > 0;
                            let severity_class = if evicted {
                                "chat-overflow-notice chat-overflow-notice--evicted"
                            } else {
                                "chat-overflow-notice chat-overflow-notice--warning"
                            };
                            view! {
                                <div
                                    class=severity_class
                                    role="status"
                                    aria-live="polite"
                                    data-testid="chat-overflow-notice"
                                >
                                    <span class="chat-overflow-notice-text">{msg}</span>
                                    <button
                                        class="btn btn-ghost btn-icon-sm chat-overflow-notice-dismiss"
                                        type="button"
                                        title="Dismiss"
                                        aria-label="Dismiss overflow notice"
                                        on:click=move |_| dismiss.dismiss_overflow_notice()
                                    >
                                        {"\u{00d7}"}
                                    </button>
                                </div>
                            }
                        })
                    }
                }
                <div class="p-4">
                    {move || {
                        let msgs = chat_state.try_get().unwrap_or_default().messages;
                        if msgs.is_empty() {
                            view! {
                                <div
                                    class="flex h-full min-h-[200px] items-center justify-center py-12"
                                    data-testid="chat-conversation-empty"
                                >
                                    <div class="text-center space-y-4 max-w-md px-4">
                                        // Conversation icon with gradient background
                                        <div class="mx-auto w-14 h-14 shrink-0 rounded-2xl bg-gradient-to-br from-primary/20 to-primary/5 flex items-center justify-center shadow-sm">
                                            <svg
                                                xmlns="http://www.w3.org/2000/svg"
                                                class="text-primary shrink-0"
                                                width="28"
                                                height="28"
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
                                            <h3 class="heading-4 text-foreground">"Start Chat"</h3>
                                            <p class="text-sm text-muted-foreground leading-relaxed">
                                                "Ask your first question, add files for context, or browse adapters."
                                            </p>
                                        </div>
                                        <div class="flex flex-wrap justify-center gap-2 pt-2">
                                            <Button
                                                size=ButtonSize::Sm
                                                on_click=Callback::new(move |_| {
                                                    #[cfg(target_arch = "wasm32")]
                                                    {
                                                        if let Some(window) = web_sys::window() {
                                                            if let Some(document) = window.document() {
                                                                if let Ok(Some(element)) = document.query_selector(
                                                                    "[data-testid='chat-input']",
                                                                ) {
                                                                    if let Some(input) =
                                                                        element.dyn_ref::<web_sys::HtmlElement>()
                                                                    {
                                                                        let _ = input.focus();
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                })
                                                data_testid="chat-conversation-start-chat".to_string()
                                            >
                                                "Start Chat"
                                            </Button>
                                            <Button
                                                variant=ButtonVariant::Outline
                                                size=ButtonSize::Sm
                                                on_click=Callback::new(move |_| show_attach_dialog.set(true))
                                                data_testid="chat-conversation-add-files".to_string()
                                            >
                                                "Add Files"
                                            </Button>
                                            <a
                                                href="/adapters"
                                                class="btn btn-ghost btn-sm"
                                                data-testid="chat-conversation-browse-adapters"
                                            >
                                                "Browse Adapters (Library)"
                                            </a>
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="space-y-5">
                                    <For
                                        each=move || message_ids.try_get().unwrap_or_default()
                                        key=|id| id.clone()
                                        children={
                                            let active_trace = active_trace;
                                            move |msg_id| {
                                                view! {
                                                    <ChatConversationMessageItem
                                                        msg_id=msg_id
                                                        active_trace=active_trace
                                                    />
                                                }
                                            }
                                        }
                                    />

                                    // Inline error indicator after messages (provides context)
                                    {move || {
                                        let state = chat_state.try_get().unwrap_or_default();
                                        let has_error = state.error.is_some();
                                        let notice = state.stream_notice.clone();
                                        let has_recovery = state.stream_recovery.is_some();

                                        if has_error {
                                            let fallback_error = state
                                                .error
                                                .as_deref()
                                                .map(str::trim)
                                                .filter(|msg| !msg.is_empty() && !msg.eq_ignore_ascii_case("error"))
                                                .map(|msg| msg.to_string());

                                            let display_msg = notice.as_ref()
                                                .map(|n| n.message.clone())
                                                .or(fallback_error)
                                                .unwrap_or_else(|| "Request failed. Retry in a moment.".to_string());

                                            let retryable = notice.as_ref()
                                                .map(|n| n.retryable)
                                                .unwrap_or(false);

                                            let is_warning = notice.as_ref()
                                                .map(|n| n.tone == StreamNoticeTone::Warning)
                                                .unwrap_or(false);

                                            let (icon_color, bg_color) = if is_warning {
                                                ("text-status-warning", "bg-warning/5 border-warning/20")
                                            } else {
                                                ("text-destructive", "bg-destructive/5 border-destructive/20")
                                            };

                                            // Contextual help based on error type
                                            let help_hint = notice.as_ref().and_then(|n| {
                                                match n.message.as_str() {
                                                    "Server is busy" => Some("The server is processing many requests. Retrying usually helps."),
                                                    "No workers available" => Some("All inference engines are busy. Try again in a moment."),
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
                {move || {
                    let has_messages = !message_ids.try_get().unwrap_or_default().is_empty();
                    if has_messages && !is_at_bottom.try_get().unwrap_or(true) {
                        Some(view! {
                            <button
                                type="button"
                                class="btn btn-outline btn-sm absolute bottom-4 right-4 inline-flex items-center gap-1.5 rounded-full border border-border bg-background/95 px-3 py-1.5 text-xs font-medium text-foreground shadow-sm hover:bg-muted/80 transition-colors"
                                on:click=move |_| scroll_to_latest.run(())
                                data-testid="chat-jump-to-latest"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 5v14m0 0l6-6m-6 6l-6-6"/>
                                </svg>
                                "Jump to latest"
                            </button>
                        })
                    } else {
                        None
                    }
                }}
            </div>

            // Trace panel (modal overlay)
            {move || {
                active_trace.try_get().flatten().map(|tid| {
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

            // Session inline notice (query param validation, etc.)
            {move || {
                session_inline_notice.try_get().flatten().map(|msg| view! {
                    <div class="rounded-md bg-warning/10 border border-warning p-3 mb-4" data-testid="chat-session-inline-notice">
                        <p class="text-sm text-warning-foreground">{msg}</p>
                    </div>
                })
            }}

            // Session confirmation state display
            {move || {
                let state = session_confirmation_state
                    .try_get()
                    .unwrap_or(SessionConfirmationState::Confirmed);
                match state {
                    SessionConfirmationState::Confirmed => None,
                    SessionConfirmationState::PendingConfirm => Some(view! {
                        <div class="rounded-md bg-warning/10 border border-warning p-3 mb-4" data-testid="chat-session-error">
                            <div class="flex flex-wrap items-center justify-between gap-2">
                                <p class="text-sm text-warning-foreground" data-testid="chat-session-state-pending">
                                    "Local draft session (not confirmed by server yet)."
                                </p>
                                <div class="flex items-center gap-3">
                                    <button
                                        type="button"
                                        class="text-sm font-medium text-primary hover:underline"
                                        on:click=move |_| retry_session_confirmation.run(())
                                        data-testid="chat-session-confirm-retry"
                                    >
                                        "Retry confirmation"
                                    </button>
                                    <a
                                        href="/chat"
                                        class="text-sm font-medium text-primary hover:underline"
                                        data-testid="chat-session-error-link"
                                    >
                                        "Start New Session"
                                    </a>
                                </div>
                            </div>
                        </div>
                    }
                        .into_any()),
                    SessionConfirmationState::NotFound => Some(view! {
                        <div class="rounded-md bg-warning/10 border border-warning p-3 mb-4" data-testid="chat-session-error">
                            <div class="flex flex-wrap items-center justify-between gap-2">
                                <p class="text-sm text-warning-foreground" data-testid="chat-session-state-not-found">
                                    "Session not found on server; link may be stale."
                                </p>
                                <div class="flex items-center gap-3">
                                    <a
                                        href="/chat"
                                        class="text-sm font-medium text-primary hover:underline"
                                        data-testid="chat-session-error-link"
                                    >
                                        "Start New Session"
                                    </a>
                                </div>
                            </div>
                        </div>
                    }
                        .into_any()),
                    SessionConfirmationState::TransientError => Some(view! {
                        <div class="rounded-md bg-warning/10 border border-warning p-3 mb-4" data-testid="chat-session-error">
                            <div class="flex flex-wrap items-center justify-between gap-2">
                                <p class="text-sm text-warning-foreground" data-testid="chat-session-state-transient">
                                    "Could not confirm session due to a temporary error."
                                </p>
                                <div class="flex items-center gap-3">
                                    <button
                                        type="button"
                                        class="text-sm font-medium text-primary hover:underline"
                                        on:click=move |_| retry_session_confirmation.run(())
                                        data-testid="chat-session-confirm-retry"
                                    >
                                        "Retry confirmation"
                                    </button>
                                    <a
                                        href="/chat"
                                        class="text-sm font-medium text-primary hover:underline"
                                        data-testid="chat-session-error-link"
                                    >
                                        "Start New Session"
                                    </a>
                                </div>
                            </div>
                        </div>
                    }
                        .into_any()),
                }
            }}

            // Error display with dismiss button
            // Uses stream_notice.message for human-readable copy, falls back to raw error
            // Retry button only appears when error is retryable AND recovery state exists
            {move || {
                let action = chat_action.clone();
                let state = chat_state.try_get().unwrap_or_default();
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
                            "No workers available" => Some("All inference engines are busy. Try again in a moment."),
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
                                                    disabled=retry_disabled
                                                    on_click=do_retry
                                                    data_testid="chat-error-retry".to_string()
                                                >
                                                    "Retry"
                                                </Button>
                                            }.into_any()
                                        } else {
                                            view! {}.into_any()
                                        }}
                                        <button
                                            class="btn btn-ghost btn-sm text-sm font-medium text-muted-foreground hover:text-foreground px-2 py-1 rounded hover:bg-muted transition-colors"
                                            on:click=move |_| action.clear_error()
                                            aria-label="Dismiss error"
                                            data-testid="chat-error-dismiss"
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
                match system_status.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Loaded(status) => {
                        if matches!(status.inference_ready, InferenceReadyState::True) {
                            view! {}.into_any()
                        } else {
                            let guidance = guidance_for(
                                status.inference_ready,
                                crate::components::inference_guidance::primary_blocker(&status.inference_blockers),
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
                                            <ButtonLink
                                                href=action.href
                                                variant=ButtonVariant::Outline
                                                size=ButtonSize::Sm
                                            >
                                                {action.label}
                                            </ButtonLink>
                                            {status_center.map(|ctx| view! {
                                                    <button
                                                        class="btn btn-link btn-xs text-xs text-muted-foreground hover:text-foreground"
                                                        on:click=move |_| ctx.open()
                                                    >
                                                        "Why?"
                                                    </button>
                                                })}
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        }
                    }
                    _ => view! {}.into_any(),
                }
            }}

            <ChatComposerPanel
                chat_state=chat_state
                base_model_label=base_model_label
                is_compact_view=Signal::derive(move || is_compact_view.try_get().unwrap_or(false))
                show_mobile_config_details=show_mobile_config_details
                message=message
                can_send=Signal::derive(move || can_send.try_get().unwrap_or(false))
                is_streaming=is_streaming
                is_loading=is_loading
                show_attach_dialog=show_attach_dialog
                on_submit=do_send
                on_cancel=do_cancel
                on_keydown=handle_keydown
            />

            <Dialog
                open=show_attach_dialog
                title="Attach data".to_string()
                description="Create training material from a file, pasted text, or this chat.".to_string()
            >
                <div class="space-y-4">
                    <div class="grid grid-cols-3 gap-2 text-xs">
                        <button
                            type="button"
                            class=move || {
                                if attach_mode.try_get().unwrap_or(AttachMode::Upload) == AttachMode::Upload {
                                    "btn btn-outline btn-sm rounded-md border border-border bg-muted px-3 py-2 text-foreground"
                                } else {
                                    "btn btn-outline btn-sm rounded-md border border-border/60 px-3 py-2 text-muted-foreground hover:text-foreground hover:bg-muted/40"
                                }
                            }
                            on:click=move |_| attach_mode.set(AttachMode::Upload)
                        >
                            "Upload file"
                        </button>
                        <button
                            type="button"
                            class=move || {
                                if attach_mode.try_get().unwrap_or(AttachMode::Upload) == AttachMode::Paste {
                                    "btn btn-outline btn-sm rounded-md border border-border bg-muted px-3 py-2 text-foreground"
                                } else {
                                    "btn btn-outline btn-sm rounded-md border border-border/60 px-3 py-2 text-muted-foreground hover:text-foreground hover:bg-muted/40"
                                }
                            }
                            on:click=move |_| attach_mode.set(AttachMode::Paste)
                        >
                            "Paste text"
                        </button>
                        <button
                            type="button"
                            class=move || {
                                if attach_mode.try_get().unwrap_or(AttachMode::Upload) == AttachMode::Chat {
                                    "btn btn-outline btn-sm rounded-md border border-border bg-muted px-3 py-2 text-foreground"
                                } else {
                                    "btn btn-outline btn-sm rounded-md border border-border/60 px-3 py-2 text-muted-foreground hover:text-foreground hover:bg-muted/40"
                                }
                            }
                            on:click=move |_| attach_mode.set(AttachMode::Chat)
                        >
                            "Use this chat"
                        </button>
                    </div>

                    {move || match attach_mode.try_get().unwrap_or(AttachMode::Upload) {
                        AttachMode::Upload => view! {
                            <div class="space-y-2">
                                <label for="chat-attach-upload-file" class="text-xs text-muted-foreground">
                                    "Select a file"
                                </label>
                                <input
                                    id="chat-attach-upload-file"
                                    type="file"
                                    class="block w-full text-xs text-muted-foreground file:mr-3 file:rounded-md file:border-0 file:bg-muted file:px-3 file:py-2 file:text-xs file:font-medium file:text-foreground hover:file:bg-muted/70"
                                    accept=".pdf,.txt,.md,.markdown"
                                    on:change=move |ev| {
                                        match selected_file_from_event(&ev) {
                                            Some(file) => match validate_attach_upload_file(&file) {
                                                Ok(()) => {
                                                    selected_file_name.set(Some(file.name()));
                                                    selected_file.set_value(Some(file));
                                                    attach_error.set(None);
                                                }
                                                Err(validation_error) => {
                                                    selected_file_name.set(None);
                                                    selected_file.set_value(None);
                                                    attach_error.set(Some(validation_error));
                                                }
                                            },
                                            None => {
                                                selected_file_name.set(None);
                                                selected_file.set_value(None);
                                            }
                                        }
                                        reset_file_input_value(&ev);
                                    }
                                />
                                <p class="text-xs text-muted-foreground">
                                    {format!(
                                        "Supported: PDF, TXT, Markdown · Max {} MB",
                                        DOCUMENT_UPLOAD_MAX_FILE_SIZE / 1024 / 1024
                                    )}
                                </p>
                                {move || selected_file_name.try_get().flatten().map(|name| view! {
                                    <div class="text-xs text-muted-foreground">
                                        {format!("Selected: {}", name)}
                                    </div>
                                })}
                            </div>
                        }.into_any(),
                        AttachMode::Paste => view! {
                            <div class="space-y-2">
                                <label for="chat-attach-paste-text" class="text-xs text-muted-foreground">
                                    "Paste text"
                                </label>
                                <Textarea
                                    id="chat-attach-paste-text".to_string()
                                    value=pasted_text
                                    placeholder="Paste training examples or notes...".to_string()
                                    rows=5
                                    class="w-full".to_string()
                                    aria_label="Paste training text".to_string()
                                />
                            </div>
                        }.into_any(),
                        AttachMode::Chat => {
                            let messages = chat_state.try_get().unwrap_or_default().messages;
                            let msg_count = messages.len();
                            let selected_count = Memo::new(move |_| selected_msg_indices.try_get().unwrap_or_default().len());

                            // Quick select: last N messages
                            let chat_state_for_quick_select = chat_state;
                            let selected_msg_indices_for_quick_select = selected_msg_indices;
                            let select_last_n = Callback::new(move |n: usize| {
                                let msgs = chat_state_for_quick_select
                                    .try_get()
                                    .unwrap_or_default()
                                    .messages;
                                let total = msgs.len();
                                let start = total.saturating_sub(n);
                                let indices: std::collections::HashSet<usize> =
                                    (start..total).collect();
                                selected_msg_indices_for_quick_select.set(indices);
                            });
                            let select_last_5 = select_last_n;
                            let select_last_10 = select_last_n;
                            let select_last_20 = select_last_n;

                            // Toggle all
                            let toggle_all = move |_| {
                                let current = selected_msg_indices.try_get().unwrap_or_default();
                                let total = chat_state.try_get().unwrap_or_default().messages.len();
                                if current.len() == total {
                                    selected_msg_indices.set(std::collections::HashSet::new());
                                } else {
                                    selected_msg_indices.set((0..total).collect());
                                }
                            };

                            view! {
                                <div class="space-y-3">
                                    <div class="flex items-center justify-between">
                                        <p class="text-xs text-muted-foreground">"Select messages"</p>
                                        <span class="text-xs text-muted-foreground">
                                            {move || format!("{} of {} selected", selected_count.try_get().unwrap_or(0), chat_state.try_get().unwrap_or_default().messages.len())}
                                        </span>
                                    </div>

                                    // Quick actions
                                    <div class="flex gap-2 flex-wrap">
                                        <button
                                            type="button"
                                            class="btn btn-outline btn-sm px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
                                            on:click=toggle_all
                                        >
                                            {move || if selected_msg_indices.try_get().unwrap_or_default().len() == chat_state.try_get().unwrap_or_default().messages.len() && !chat_state.try_get().unwrap_or_default().messages.is_empty() {
                                                "Deselect all"
                                            } else {
                                                "Select all"
                                            }}
                                        </button>
                                        <button
                                            type="button"
                                            class="btn btn-outline btn-sm px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
                                            on:click=move |_| select_last_5.run(5)
                                        >
                                            "Last 5"
                                        </button>
                                        <button
                                            type="button"
                                            class="btn btn-outline btn-sm px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
                                            on:click=move |_| select_last_10.run(10)
                                        >
                                            "Last 10"
                                        </button>
                                        <button
                                            type="button"
                                            class="btn btn-outline btn-sm px-2 py-1 text-xs rounded border border-border hover:bg-muted/50"
                                            on:click=move |_| select_last_20.run(20)
                                        >
                                            "Last 20"
                                        </button>
                                    </div>

                                    // Message list with checkboxes
                                    {if msg_count == 0 {
                                        view! {
                                            <p class="text-xs text-muted-foreground py-4 text-center">
                                                "No messages in this chat session."
                                            </p>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div class="max-h-48 overflow-y-auto border border-border rounded-md">
                                                {messages.into_iter().enumerate().map(|(idx, msg)| {
                                                    let is_checked = Memo::new(move |_| selected_msg_indices.try_get().unwrap_or_default().contains(&idx));
                                                    let role_badge = if msg.role == "user" { "U" } else { "A" };
                                                    let content_preview: String = msg.content.chars().take(60).collect::<String>()
                                                        + if msg.content.len() > 60 { "..." } else { "" };
                                                    let toggle_msg = move |checked: bool| {
                                                        selected_msg_indices.update(|set| {
                                                            if checked {
                                                                set.insert(idx);
                                                            } else {
                                                                set.remove(&idx);
                                                            }
                                                        });
                                                    };
                                                    view! {
                                                        <div class="flex items-start gap-2 px-3 py-2 border-b border-border/50 last:border-b-0 hover:bg-muted/30">
                                                            <Checkbox
                                                                checked=Signal::derive(move || is_checked.try_get().unwrap_or(false))
                                                                on_change=Callback::new(toggle_msg)
                                                                aria_label=format!("Select message {}", idx + 1)
                                                            />
                                                            <span class=move || format!(
                                                                "shrink-0 w-5 h-5 rounded text-xs flex items-center justify-center {}",
                                                                if msg.role == "user" { "bg-primary/20 text-primary" } else { "bg-muted text-muted-foreground" }
                                                            )>
                                                                {role_badge}
                                                            </span>
                                                            <span class="text-xs text-foreground/80 line-clamp-2 flex-1">
                                                                {content_preview}
                                                            </span>
                                                        </div>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        }.into_any()
                                    }}
                                </div>
                            }.into_any()
                        },
                    }}

                    {move || attach_error.try_get().flatten().map(|msg| view! {
                        <div class="text-xs text-destructive">{msg}</div>
                    })}
                    {move || attach_status.try_get().flatten().map(|msg| view! {
                        <div class="text-xs text-muted-foreground">{msg}</div>
                    })}

                    <div class="flex justify-end gap-2 pt-2 border-t border-border">
                        <Button
                            variant=ButtonVariant::Outline
                            disabled=Signal::derive(move || attach_busy.try_get().unwrap_or(false))
                            on_click=Callback::new(move |_| show_attach_dialog.set(false))
                        >
                            "Cancel"
                        </Button>
                        <Button
                            variant=ButtonVariant::Primary
                            loading=Signal::derive(move || attach_busy.try_get().unwrap_or(false))
                            disabled=Signal::derive(move || attach_busy.try_get().unwrap_or(false))
                            on_click=create_draft
                        >
                            "Create draft"
                        </Button>
                    </div>
                </div>
            </Dialog>
        </div>
    }
}
