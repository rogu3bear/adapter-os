//! Dashboard page
//!
//! Calm Home: trust anchors visible, Guided Flow as hero, minimal operational noise.
//! Full metrics and infrastructure live at /system.

use crate::api::SseState;
use crate::boot_log;
use crate::components::inference_guidance::{guidance_for, primary_blocker};
use crate::components::status_center::use_status_center;
use crate::components::{
    Button, ButtonLink, ButtonSize, ButtonVariant, Card, ErrorDisplay, IconCheckCircle, IconPlay,
    PageScaffold, PageScaffoldActions, PageScaffoldPrimaryAction, PageScaffoldStatus,
    SkeletonStatsGrid, StatusColor, StatusIconBox, StatusIndicator, StatusVariant,
};
use crate::hooks::{
    use_live_system_metrics, use_sse_notifications, use_system_status, LoadingState,
};
use crate::utils::format_relative_time;
use adapteros_api_types::{
    InferenceReadyState, StatusIndicator as ApiStatusIndicator, SystemStatusResponse,
};
use leptos::prelude::*;

/// Dashboard page
#[component]
pub fn Dashboard() -> impl IntoView {
    boot_log("route", "Dashboard rendered");

    let (status, refetch) = use_system_status();
    let live_metrics = use_live_system_metrics();
    use_sse_notifications(live_metrics.sse_status.read_only());

    let logged_first_status = StoredValue::new(false);
    Effect::new(move || {
        let Some(current) = status.try_get() else {
            return;
        };
        if let LoadingState::Loaded(_) = current {
            if !logged_first_status.get_value() {
                logged_first_status.set_value(true);
                boot_log("api", "first /v1/system/status success");
            }
        }
    });

    view! {
        <PageScaffold
            title="Home"
            subtitle="Start chat, build adapters, and review evidence with clear steps."
            full_width=true
        >
            <PageScaffoldStatus slot>
                <SseIndicator state=live_metrics.sse_status/>
            </PageScaffoldStatus>
            <PageScaffoldPrimaryAction slot>
                <ButtonLink
                    href="/chat"
                    variant=ButtonVariant::Primary
                    size=ButtonSize::Sm
                >
                    "Start Chat"
                </ButtonLink>
            </PageScaffoldPrimaryAction>
            <PageScaffoldActions slot>
                <Button
                    variant=ButtonVariant::Secondary
                    on_click=Callback::new(move |_| refetch.run(()))
                    aria_label="Refresh dashboard data".to_string()
                >
                    "Refresh"
                </Button>
            </PageScaffoldActions>

            {move || {
                match status.try_get().unwrap_or(LoadingState::Loading) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="mt-4 grid gap-3 sm:grid-cols-2">
                                <div class="h-24 rounded-lg border border-border animate-pulse bg-muted/20"/>
                                <div class="h-24 rounded-lg border border-border animate-pulse bg-muted/20"/>
                            </div>
                            <SkeletonStatsGrid count=2/>
                            <div class="mt-3 grid gap-2">
                                <div class="h-8 rounded-lg border border-border animate-pulse bg-muted/20"/>
                                <div class="h-8 rounded-lg border border-border animate-pulse bg-muted/20"/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! {
                            <DashboardContent status=data/>
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| refetch.run(()))
                            />
                        }.into_any()
                    }
                }
            }}
        </PageScaffold>
    }
}

/// SSE connection status indicator
#[component]
fn SseIndicator(state: RwSignal<SseState>) -> impl IntoView {
    view! {
        <div class="flex items-center gap-2">
            {move || {
                let current_state = state.try_get().unwrap_or(SseState::Disconnected);
                let (color, label) = match current_state {
                    SseState::Connected => (StatusColor::Green, "Live"),
                    SseState::Connecting => (StatusColor::Yellow, "Connecting"),
                    SseState::Error => (StatusColor::Red, "Error"),
                    SseState::CircuitOpen => (StatusColor::Red, "Circuit Open"),
                    SseState::Disconnected => (StatusColor::Gray, "Offline"),
                };

                view! {
                    <StatusIndicator
                        color=color
                        pulsing={current_state == SseState::Connected}
                        label=label.to_string()
                    />
                }
            }}
        </div>
    }
}

#[component]
fn DashboardContent(status: SystemStatusResponse) -> impl IntoView {
    let status_center = use_status_center();
    let is_ready = matches!(status.readiness.overall, ApiStatusIndicator::Ready);
    let db_status = matches!(status.readiness.checks.db.status, ApiStatusIndicator::Ready);
    let inference_needs_attention = !matches!(status.inference_ready, InferenceReadyState::True);
    let inference_guidance = inference_needs_attention.then(|| {
        guidance_for(
            status.inference_ready,
            primary_blocker(&status.inference_blockers),
        )
    });

    let inference_text = match status.inference_ready {
        InferenceReadyState::True => "Ready",
        InferenceReadyState::False => "Not Ready",
        InferenceReadyState::Unknown => "Unknown",
    };

    view! {
        <JourneyFlowSection />

        <div class="mt-4 grid gap-4 sm:grid-cols-2">
            <Card title="System Status".to_string()>
                <div class="flex items-center gap-3">
                    <StatusIconBox status=StatusVariant::from_bool(is_ready)>
                        <IconCheckCircle class="h-5 w-5".to_string() />
                    </StatusIconBox>
                    <div>
                        <StatusIndicator
                            color=StatusVariant::from_bool(is_ready).to_status_color()
                            pulsing=is_ready
                            label=if is_ready { "Ready".to_string() } else { "Not Ready".to_string() }
                        />
                        <p class="text-xs text-muted-foreground mt-1">{format_relative_time(&status.timestamp)}</p>
                    </div>
                </div>
            </Card>

            <Card title="Chat".to_string()>
                <div class="flex items-center gap-3">
                    <StatusIconBox status=match status.inference_ready {
                        InferenceReadyState::True => StatusVariant::Success,
                        InferenceReadyState::False => StatusVariant::Error,
                        InferenceReadyState::Unknown => StatusVariant::Warning,
                    }>
                        <IconPlay class="h-5 w-5".to_string() />
                    </StatusIconBox>
                    <div>
                        <div class="text-2xl font-bold">{inference_text}</div>
                        <p class="text-xs text-muted-foreground">"Prompt readiness"</p>
                        {if let Some(guidance) = inference_guidance {
                            let action = guidance.action;
                            Some(view! {
                                <div class="mt-2 space-y-2">
                                    <p class="text-xs text-muted-foreground">{guidance.reason}</p>
                                    <div class="flex flex-wrap items-center gap-2">
                                        <ButtonLink
                                            href=action.href
                                            variant=ButtonVariant::Outline
                                            size=ButtonSize::Sm
                                        >
                                            {action.label}
                                        </ButtonLink>
                                        {status_center.map(|ctx| view! {
                                            <button
                                                class="text-xs text-muted-foreground hover:text-foreground"
                                                on:click=move |_| ctx.open()
                                            >
                                                "Why?"
                                            </button>
                                        })}
                                    </div>
                                </div>
                            })
                        } else {
                            None
                        }}
                    </div>
                </div>
            </Card>
        </div>

        <div class="mt-3 flex items-center justify-between rounded-lg border border-border/60 bg-muted/20 px-3 py-2">
            <StatusIndicator
                color=StatusVariant::from_bool(db_status).to_status_color()
                pulsing=db_status
                label=if db_status {
                    "System services connected".to_string()
                } else {
                    "System services need attention".to_string()
                }
            />
            <a href="/system" class="text-xs font-medium text-primary hover:underline">
                "View System"
            </a>
        </div>
    }
}

#[component]
fn JourneyFlowSection() -> impl IntoView {
    view! {
        <Card title="Quick Start".to_string() class="mt-4">
            <p class="text-sm text-muted-foreground mb-4">
                "Use AdapterOS as a chat that can build adapters and produce proof."
            </p>
            <div class="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
                <JourneyStep
                    step="Action 1"
                    title="Start Chat"
                    body="Open chat, ask a question, and iterate quickly."
                    href="/chat"
                    cta="Start Chat"
                />
                <JourneyStep
                    step="Action 2"
                    title="Create Adapter"
                    body="Add your files, name the adapter, and start training."
                    href="/training?open_wizard=1"
                    cta="Create Adapter"
                />
                <JourneyStep
                    step="Action 3"
                    title="View Evidence"
                    body="Review execution records, receipts, and replay results."
                    href="/runs"
                    cta="View Evidence"
                />
            </div>
        </Card>
    }
}

#[component]
fn JourneyStep(
    step: &'static str,
    title: &'static str,
    body: &'static str,
    href: &'static str,
    cta: &'static str,
) -> impl IntoView {
    let cta_aria = format!("{step}: {title}. {cta}");

    view! {
        <div class="rounded-lg border border-border/60 bg-card/60 p-3 space-y-2">
            <p class="text-[11px] uppercase tracking-wide text-muted-foreground">{step}</p>
            <p class="text-sm font-semibold">{title}</p>
            <p class="text-xs text-muted-foreground">{body}</p>
            <ButtonLink
                href=href
                variant=ButtonVariant::Outline
                size=ButtonSize::Sm
                aria_label=cta_aria
            >
                {cta}
            </ButtonLink>
        </div>
    }
}
