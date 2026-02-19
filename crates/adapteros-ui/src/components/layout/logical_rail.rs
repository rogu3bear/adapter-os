//! LogicalControlRail - Slim status bar for inference readiness.
//!
//! Shows inference ready/blocked + primary blocker. Full contract details
//! live on the System page. Slimmed per UI_OVERBUILT_AUDIT.

use super::topbar::short_fingerprint;
use crate::components::inference_guidance::{guidance_for, primary_blocker};
use crate::components::Badge;
use crate::hooks::{use_startup_health, use_system_status, LoadingState};
use adapteros_api_types::{InferenceBlocker, InferenceReadyState, SystemStatusResponse};
use leptos::prelude::*;
#[derive(Debug, Clone, PartialEq)]
struct LogicSnapshot {
    fingerprint: String,
    primary_blocker: Option<InferenceBlocker>,
    inference_ready: InferenceReadyState,
}

impl LogicSnapshot {
    fn from_status(status: &SystemStatusResponse) -> Self {
        Self {
            fingerprint: super::topbar::configuration_fingerprint(status),
            primary_blocker: primary_blocker(&status.inference_blockers).cloned(),
            inference_ready: status.inference_ready,
        }
    }
}

#[component]
pub fn LogicalControlRail() -> impl IntoView {
    let (system_status, refetch_status) = use_system_status();
    let _startup_health = use_startup_health();

    let on_refresh = move |_| {
        refetch_status.run(());
    };

    view! {
        <section class="logic-rail logic-rail--slim" aria-label="Inference status" aria-live="polite">
            {move || match system_status.get() {
                LoadingState::Loaded(status) => {
                    let snapshot = LogicSnapshot::from_status(&status);
                    let guidance = guidance_for(
                        status.inference_ready,
                        snapshot.primary_blocker.as_ref(),
                    );
                    view! {
                        <div class="logic-rail__slim">
                            <div class="logic-rail__slim-status">
                                <Badge variant=inference_badge_variant(snapshot.inference_ready)>
                                    {inference_label(snapshot.inference_ready)}
                                </Badge>
                                {snapshot.primary_blocker.as_ref().map(|b| view! {
                                    <span class="logic-rail__slim-blocker">
                                        {blocker_label(b)}
                                    </span>
                                })}
                            </div>
                            <div class="logic-rail__slim-meta">
                                <span class="logic-rail__slim-fingerprint" title=snapshot.fingerprint.clone()>
                                    {short_fingerprint(&snapshot.fingerprint)}
                                </span>
                                <a class="logic-rail__slim-link" href=guidance.action.href>
                                    {guidance.action.label}
                                </a>
                                <a class="logic-rail__slim-link" href="/system">
                                    "System"
                                </a>
                                <button class="logic-rail__slim-link" type="button" on:click=on_refresh>
                                    "Refresh"
                                </button>
                            </div>
                        </div>
                    }.into_any()
                }
                LoadingState::Error(_err) => view! {
                    <div class="logic-rail__slim logic-rail__slim--error">
                        <span class="text-status-error">"Status unavailable"</span>
                        <a class="logic-rail__slim-link" href="/system">
                            "Open System"
                        </a>
                    </div>
                }.into_any(),
                LoadingState::Idle | LoadingState::Loading => view! {
                    <div class="logic-rail__slim">
                        <span class="text-muted-foreground text-sm">"Loading…"</span>
                    </div>
                }.into_any(),
            }}
        </section>
    }
}

fn inference_label(state: InferenceReadyState) -> &'static str {
    match state {
        InferenceReadyState::True => "Ready",
        InferenceReadyState::False => "Blocked",
        InferenceReadyState::Unknown => "Unknown",
    }
}

fn inference_badge_variant(state: InferenceReadyState) -> crate::components::BadgeVariant {
    use crate::components::BadgeVariant;
    match state {
        InferenceReadyState::True => BadgeVariant::Success,
        InferenceReadyState::False => BadgeVariant::Warning,
        InferenceReadyState::Unknown => BadgeVariant::Secondary,
    }
}

fn blocker_label(blocker: &InferenceBlocker) -> &'static str {
    match blocker {
        InferenceBlocker::DatabaseUnavailable => "Core services unavailable",
        InferenceBlocker::WorkerMissing => "No inference engines online",
        InferenceBlocker::NoModelLoaded => "No base model active",
        InferenceBlocker::ActiveModelMismatch => "Base model mismatch",
        InferenceBlocker::TelemetryDegraded => "Telemetry degraded",
        InferenceBlocker::SystemBooting => "Kernel boot in progress",
        InferenceBlocker::BootFailed => "Kernel boot failed",
    }
}
