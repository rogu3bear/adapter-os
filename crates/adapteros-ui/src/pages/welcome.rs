//! Welcome / first-run page
//!
//! Shown when AdapterOS detects a fresh installation (no models loaded,
//! no workers registered). Guides the operator through initial setup.

use crate::api::ApiClient;
use crate::components::{Button, ButtonLink, ButtonSize, ButtonVariant, PageScaffold, Spinner};
use crate::hooks::{use_api_resource, LoadingState};
use adapteros_api_types::{
    InferenceBlocker, InferenceReadyState, StatusIndicator, SystemStatusResponse,
};
use leptos::prelude::*;
use std::sync::Arc;

/// A single setup checklist item.
struct CheckItem {
    label: &'static str,
    status: CheckStatus,
    hint: &'static str,
    action_label: Option<&'static str>,
    action_href: Option<&'static str>,
}

#[derive(Clone, Copy, PartialEq)]
enum CheckStatus {
    Ready,
    Issue,
    Unknown,
}

impl CheckStatus {
    fn icon_path(self) -> &'static str {
        match self {
            Self::Ready => "M5 13l4 4L19 7",
            Self::Issue => "M12 9v4m0 4h.01",
            Self::Unknown => "M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01",
        }
    }
    fn css_class(self) -> &'static str {
        match self {
            Self::Ready => "welcome-check-ready",
            Self::Issue => "welcome-check-issue",
            Self::Unknown => "welcome-check-unknown",
        }
    }
}

fn derive_checklist(status: &SystemStatusResponse) -> Vec<CheckItem> {
    let db_ok = status.readiness.checks.db.status == StatusIndicator::Ready;
    let migrations_ok = status.readiness.checks.migrations.status == StatusIndicator::Ready;
    let workers_ok = status.readiness.checks.workers.status == StatusIndicator::Ready;
    let models_ok = status.readiness.checks.models.status == StatusIndicator::Ready;

    let model_count = status
        .kernel
        .as_ref()
        .and_then(|k| k.models.as_ref())
        .and_then(|m| m.total)
        .unwrap_or(0);

    let adapter_count = status
        .kernel
        .as_ref()
        .and_then(|k| k.adapters.as_ref())
        .and_then(|a| a.total_active)
        .unwrap_or(0);

    let inference_ready = status.inference_ready == InferenceReadyState::True;

    vec![
        CheckItem {
            label: "Database",
            status: if db_ok && migrations_ok {
                CheckStatus::Ready
            } else {
                CheckStatus::Issue
            },
            hint: if db_ok && migrations_ok {
                "Connected, migrations current"
            } else if db_ok {
                "Connected, but migrations need attention"
            } else {
                "Run: ./aosctl db migrate"
            },
            action_label: None,
            action_href: None,
        },
        CheckItem {
            label: "Workers",
            status: if workers_ok {
                CheckStatus::Ready
            } else {
                CheckStatus::Issue
            },
            hint: if workers_ok {
                "Worker connected and ready"
            } else {
                "A worker runs models. Connect one to continue"
            },
            action_label: if workers_ok {
                None
            } else {
                Some("Set up worker")
            },
            action_href: if workers_ok { None } else { Some("/workers") },
        },
        CheckItem {
            label: "Models",
            status: if models_ok {
                CheckStatus::Ready
            } else if model_count > 0 {
                CheckStatus::Issue
            } else {
                CheckStatus::Issue
            },
            hint: if models_ok {
                "Model loaded on a worker"
            } else if model_count > 0 {
                "Model added, but not loaded on a worker"
            } else {
                "Next: add a model after a worker is online"
            },
            action_label: Some("Open models"),
            action_href: Some("/models"),
        },
        CheckItem {
            label: "Adapters",
            status: if adapter_count > 0 {
                CheckStatus::Ready
            } else {
                CheckStatus::Unknown
            },
            hint: if adapter_count > 0 {
                "Adapters available in workspace"
            } else {
                "Optional \u{2014} train or register an adapter"
            },
            action_label: Some("Adapters"),
            action_href: Some("/adapters"),
        },
        CheckItem {
            label: "Inference",
            status: if inference_ready {
                CheckStatus::Ready
            } else {
                CheckStatus::Issue
            },
            hint: if inference_ready {
                "System ready for inference"
            } else {
                primary_blocker_hint(&status.inference_blockers)
            },
            action_label: if inference_ready {
                Some("Open chat")
            } else {
                Some("See blockers")
            },
            action_href: if inference_ready {
                Some("/chat")
            } else {
                Some("/system")
            },
        },
    ]
}

fn primary_blocker_hint(blockers: &[InferenceBlocker]) -> &'static str {
    let primary = blockers.iter().min_by_key(|b| match b {
        InferenceBlocker::BootFailed => 0,
        InferenceBlocker::SystemBooting => 1,
        InferenceBlocker::DatabaseUnavailable => 2,
        InferenceBlocker::WorkerMissing => 3,
        InferenceBlocker::NoModelLoaded => 4,
        InferenceBlocker::ActiveModelMismatch => 5,
        InferenceBlocker::TelemetryDegraded => 6,
    });
    match primary {
        Some(InferenceBlocker::BootFailed) => "Boot failed \u{2014} check server logs",
        Some(InferenceBlocker::SystemBooting) => "System is still booting",
        Some(InferenceBlocker::DatabaseUnavailable) => "Database unavailable",
        Some(InferenceBlocker::WorkerMissing) => "No worker connected",
        Some(InferenceBlocker::NoModelLoaded) => "No model loaded on a worker",
        Some(InferenceBlocker::ActiveModelMismatch) => "Loaded model does not match selection",
        Some(InferenceBlocker::TelemetryDegraded) => "Telemetry degraded (non-blocking)",
        None => "Resolve issues above to enable inference",
    }
}

/// Welcome page for first-run setup guidance.
#[component]
pub fn Welcome() -> impl IntoView {
    let (status, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.system_status().await });

    let on_refresh = Callback::new(move |_| refetch.run(()));

    view! {
        <PageScaffold
            title="Welcome"
            subtitle="First-run setup"
        >
            <div class="welcome-container">
                <div class="welcome-card">
                    <div class="welcome-header">
                        <svg class="welcome-logo" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M21 16V8a2 2 0 00-1-1.73l-7-4a2 2 0 00-2 0l-7 4A2 2 0 003 8v8a2 2 0 001 1.73l7 4a2 2 0 002 0l7-4A2 2 0 0021 16z"/>
                            <polyline points="7.5 4.21 12 6.81 16.5 4.21"/>
                            <polyline points="7.5 19.79 7.5 14.6 3 12"/>
                            <polyline points="21 12 16.5 14.6 16.5 19.79"/>
                            <polyline points="3.27 6.96 12 12.01 20.73 6.96"/>
                            <line x1="12" y1="22.08" x2="12" y2="12"/>
                        </svg>
                        <h2 class="welcome-title">"Welcome to AdapterOS"</h2>
                        <p class="welcome-subtitle">
                            "A worker runs your model and handles requests. "
                            "Connect a worker, then load a model."
                        </p>
                    </div>

                    {move || {
                        match status.get() {
                            LoadingState::Idle | LoadingState::Loading => view! {
                                <div class="welcome-loading">
                                    <Spinner />
                                    <span class="text-sm text-muted-foreground">"Checking system status\u{2026}"</span>
                                </div>
                            }.into_any(),
                            LoadingState::Error(_) => view! {
                                <div class="welcome-checklist">
                                    <div class="welcome-error">
                                        <svg class="welcome-check-icon" style="color: var(--color-destructive); flex-shrink: 0;" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                            <circle cx="12" cy="12" r="10"/>
                                            <line x1="15" y1="9" x2="9" y2="15"/>
                                            <line x1="9" y1="9" x2="15" y2="15"/>
                                        </svg>
                                        <div>
                                            <p class="text-sm font-semibold">"Could not reach the backend"</p>
                                            <p class="text-xs text-muted-foreground">
                                                "Make sure the server is running: " <code>"./start"</code>
                                            </p>
                                        </div>
                                    </div>
                                    <Button
                                        variant=ButtonVariant::Outline
                                        size=ButtonSize::Sm
                                        class="mt-4".to_string()
                                        on_click=on_refresh
                                    >
                                        "Retry"
                                    </Button>
                                </div>
                            }.into_any(),
                            LoadingState::Loaded(ref s) => {
                                let checklist = derive_checklist(s);
                                let all_ready = checklist.iter().all(|c| c.status == CheckStatus::Ready);
                                let ready_count = checklist.iter().filter(|c| c.status == CheckStatus::Ready).count();
                                let total = checklist.len();
                                view! {
                                    <div class="welcome-checklist">
                                        <div class="welcome-progress-bar">
                                            <div class="welcome-progress-fill" style=format!("width: {}%", ready_count * 100 / total) />
                                        </div>
                                        <p class="welcome-progress-label">
                                            {format!("{} of {} checks passing", ready_count, total)}
                                        </p>

                                        <ul class="welcome-checks">
                                            {checklist.into_iter().map(|item| {
                                                let status_class = item.status.css_class();
                                                let icon = item.status.icon_path();
                                                view! {
                                                    <li class=format!("welcome-check-item {}", status_class)>
                                                        <svg class="welcome-check-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                                            <path d=icon />
                                                        </svg>
                                                        <div class="welcome-check-content">
                                                            <span class="welcome-check-label">{item.label}</span>
                                                            <span class="welcome-check-hint">{item.hint}</span>
                                                        </div>
                                                        {item.action_href.map(|href| {
                                                            let label = item.action_label.unwrap_or("View");
                                                            view! {
                                                                <a href=href class="welcome-check-action">{label}</a>
                                                            }
                                                        })}
                                                    </li>
                                                }
                                            }).collect_view()}
                                        </ul>

                                        {if all_ready {
                                            Some(view! {
                                                <div class="welcome-ready">
                                                    <svg class="welcome-ready-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                                        <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/>
                                                        <polyline points="22 4 12 14.01 9 11.01"/>
                                                    </svg>
                                                    <p class="welcome-ready-text">"System is ready for inference"</p>
                                                    <ButtonLink
                                                        href="/chat"
                                                        variant=ButtonVariant::Primary
                                                        size=ButtonSize::Md
                                                        class="mt-3".to_string()
                                                    >
                                                        "Open chat"
                                                    </ButtonLink>
                                                </div>
                                            })
                                        } else {
                                            None
                                        }}
                                    </div>
                                }.into_any()
                            },
                        }
                    }}

                    <div class="welcome-skip">
                        <a href="/" class="welcome-skip-link">
                            "Skip to Dashboard"
                        </a>
                    </div>
                </div>
            </div>
        </PageScaffold>
    }
}
