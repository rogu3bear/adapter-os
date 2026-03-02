//! Welcome / first-run page
//!
//! Shown when AdapterOS detects a fresh installation (no models loaded,
//! no workers registered). Guides the operator through initial setup
//! via an interactive 4-step wizard.

use crate::components::{Button, ButtonLink, ButtonSize, ButtonVariant, PageScaffold, Spinner};
use crate::hooks::{use_polling, use_system_status, LoadingState};
use crate::signals::{use_refetch_signal, RefetchTopic};
#[cfg(target_arch = "wasm32")]
use adapteros_api_types::SetupSeedModelsRequest;
use adapteros_api_types::{
    InferenceReadyState, SetupDiscoveredModel, StatusIndicator, SystemStatusResponse,
};
use leptos::prelude::*;
#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Legacy checklist (kept for derive_checklist backward compat)
// ---------------------------------------------------------------------------

/// A single setup checklist item.
struct CheckItem {
    status: CheckStatus,
}

#[derive(Clone, Copy, PartialEq)]
enum CheckStatus {
    Ready,
    Issue,
    Unknown,
}

fn derive_checklist(status: &SystemStatusResponse) -> Vec<CheckItem> {
    let db_ok = status.readiness.checks.db.status == StatusIndicator::Ready;
    let migrations_ok = status.readiness.checks.migrations.status == StatusIndicator::Ready;
    let workers_ok = status.readiness.checks.workers.status == StatusIndicator::Ready;
    let models_ok = status.readiness.checks.models.status == StatusIndicator::Ready;

    let adapter_count = status
        .kernel
        .as_ref()
        .and_then(|k| k.adapters.as_ref())
        .and_then(|a| a.total_active)
        .unwrap_or(0);

    let inference_ready = status.inference_ready == InferenceReadyState::True;

    vec![
        CheckItem {
            status: if db_ok && migrations_ok {
                CheckStatus::Ready
            } else {
                CheckStatus::Issue
            },
        },
        CheckItem {
            status: if workers_ok {
                CheckStatus::Ready
            } else {
                CheckStatus::Issue
            },
        },
        CheckItem {
            status: if models_ok {
                CheckStatus::Ready
            } else {
                CheckStatus::Issue
            },
        },
        CheckItem {
            status: if adapter_count > 0 {
                CheckStatus::Ready
            } else {
                CheckStatus::Unknown
            },
        },
        CheckItem {
            status: if inference_ready {
                CheckStatus::Ready
            } else {
                CheckStatus::Issue
            },
        },
    ]
}

// ---------------------------------------------------------------------------
// Wizard state machine
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum WizardStep {
    Database,
    Worker,
    Models,
    Ready,
}

impl WizardStep {
    fn index(self) -> usize {
        match self {
            Self::Database => 0,
            Self::Worker => 1,
            Self::Models => 2,
            Self::Ready => 3,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Database => "System Storage",
            Self::Worker => "Compute Worker",
            Self::Models => "Base Model",
            Self::Ready => "Start Using AdapterOS",
        }
    }

    const ALL: [WizardStep; 4] = [
        WizardStep::Database,
        WizardStep::Worker,
        WizardStep::Models,
        WizardStep::Ready,
    ];
}

/// Determine the current wizard step from system status.
fn wizard_step_from_status(status: &SystemStatusResponse) -> WizardStep {
    let db_ok = status.readiness.checks.db.status == StatusIndicator::Ready
        && status.readiness.checks.migrations.status == StatusIndicator::Ready;
    if !db_ok {
        return WizardStep::Database;
    }

    let workers_ok = status.readiness.checks.workers.status == StatusIndicator::Ready;
    if !workers_ok {
        return WizardStep::Worker;
    }

    let models_ok = status.readiness.checks.models.status == StatusIndicator::Ready;
    if !models_ok {
        return WizardStep::Models;
    }

    WizardStep::Ready
}

// ---------------------------------------------------------------------------
// Wizard progress indicator
// ---------------------------------------------------------------------------

#[component]
fn WizardProgress(current_step: WizardStep) -> impl IntoView {
    view! {
        <div class="wizard-progress">
            {WizardStep::ALL.into_iter().map(|step| {
                let is_complete = step.index() < current_step.index();
                let is_active = step == current_step;
                let class = if is_complete {
                    "wizard-step wizard-step-complete"
                } else if is_active {
                    "wizard-step wizard-step-active"
                } else {
                    "wizard-step"
                };
                let step_num = step.index() + 1;
                view! {
                    <div class=class>
                        <div class="wizard-step-circle">
                            {if is_complete {
                                view! {
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                                        <path d="M5 13l4 4L19 7"/>
                                    </svg>
                                }.into_any()
                            } else {
                                view! { <span>{step_num}</span> }.into_any()
                            }}
                        </div>
                        <span class="wizard-step-label">{step.label()}</span>
                    </div>
                }
            }).collect_view()}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Individual wizard step views
// ---------------------------------------------------------------------------

#[component]
fn DatabaseStep(
    db_ok: bool,
    #[prop(into)] on_migrate: Callback<()>,
    migrating: ReadSignal<bool>,
    migrate_error: ReadSignal<Option<String>>,
) -> impl IntoView {
    let is_migrating = move || migrating.try_get().unwrap_or(false);
    let error_msg = move || migrate_error.try_get().flatten();
    view! {
        <div class="wizard-action-area">
            <h3 class="wizard-step-title">"Prepare System Storage"</h3>
            <Show
                when=move || db_ok
                fallback=move || view! {
                    <p class="wizard-step-desc">
                        "AdapterOS needs to apply setup updates before it can run safely."
                    </p>
                    <Show
                        when=is_migrating
                        fallback=move || view! {
                            <Button
                                variant=ButtonVariant::Primary
                                size=ButtonSize::Md
                                on_click=Callback::new(move |_| on_migrate.run(()))
                            >
                                "Apply Setup Updates"
                            </Button>
                        }
                    >
                        <div class="wizard-inline-spinner">
                            <Spinner />
                            <span>"Applying setup updates..."</span>
                        </div>
                    </Show>
                    {move || error_msg().map(|e| view! {
                        <p class="wizard-error">{e}</p>
                    })}
                }
            >
                <div class="wizard-step-success">
                    <svg class="wizard-success-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/>
                        <polyline points="22 4 12 14.01 9 11.01"/>
                    </svg>
                    <span>"Storage connected and up to date"</span>
                </div>
            </Show>
        </div>
    }
}

#[component]
fn WorkerStep(worker_connected: bool) -> impl IntoView {
    view! {
        <div class="wizard-action-area">
            <h3 class="wizard-step-title">"Connect a Compute Worker"</h3>
            <Show
                when=move || worker_connected
                fallback=move || view! {
                    <p class="wizard-step-desc">
                        "A compute worker powers conversations. Start one in a terminal:"
                    </p>
                    <code class="wizard-code-block">"./start worker"</code>
                    <p class="wizard-step-hint">"Waiting for a worker to join..."</p>
                    <ButtonLink
                        href="/workers"
                        variant=ButtonVariant::Outline
                        size=ButtonSize::Sm
                        class="mt-2".to_string()
                    >
                        "Open Worker Status"
                    </ButtonLink>
                }
            >
                <div class="wizard-step-success">
                    <svg class="wizard-success-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/>
                        <polyline points="22 4 12 14.01 9 11.01"/>
                    </svg>
                    <span>"Compute worker connected and healthy"</span>
                </div>
            </Show>
        </div>
    }
}

#[component]
fn ModelsStep(
    models_seeded: bool,
    model_count: i64,
    discovered_models: ReadSignal<Vec<SetupDiscoveredModel>>,
    selected_model_paths: ReadSignal<Vec<String>>,
    discovering: ReadSignal<bool>,
    seeding: ReadSignal<bool>,
    discover_error: ReadSignal<Option<String>>,
    seed_error: ReadSignal<Option<String>>,
    seed_message: ReadSignal<Option<String>>,
    #[prop(into)] on_discover: Callback<()>,
    #[prop(into)] on_toggle_model: Callback<String>,
    #[prop(into)] on_seed_selected: Callback<()>,
) -> impl IntoView {
    let discover_error_msg = move || discover_error.try_get().flatten();
    let seed_error_msg = move || seed_error.try_get().flatten();
    let seed_message_msg = move || seed_message.try_get().flatten();

    view! {
        <div class="wizard-action-area">
            <h3 class="wizard-step-title">"Register a Base Model"</h3>
            <Show
                when=move || models_seeded
                fallback=move || view! {
                    <p class="wizard-step-desc">
                        "Find base model folders and register the ones you want available."
                    </p>
                    <div class="wizard-ready-actions">
                        <Button
                            variant=ButtonVariant::Outline
                            size=ButtonSize::Sm
                            loading=Signal::derive(move || discovering.try_get().unwrap_or(false))
                            disabled=Signal::derive(move || seeding.try_get().unwrap_or(false))
                            on_click=Callback::new(move |_| on_discover.run(()))
                        >
                            "Find Base Models"
                        </Button>
                        <Button
                            variant=ButtonVariant::Primary
                            size=ButtonSize::Sm
                            loading=Signal::derive(move || seeding.try_get().unwrap_or(false))
                            disabled=Signal::derive(move || {
                                discovering.try_get().unwrap_or(false)
                                    || seeding.try_get().unwrap_or(false)
                                    || selected_model_paths
                                        .try_get()
                                        .map(|paths| paths.is_empty())
                                        .unwrap_or(true)
                            })
                            on_click=Callback::new(move |_| on_seed_selected.run(()))
                        >
                            "Register Selected"
                        </Button>
                    </div>
                    {move || discover_error_msg().map(|e| view! {
                        <p class="wizard-error">{e}</p>
                    })}
                    {move || seed_error_msg().map(|e| view! {
                        <p class="wizard-error">{e}</p>
                    })}
                    {move || seed_message_msg().map(|m| view! {
                        <p class="wizard-step-hint">{m}</p>
                    })}
                    <Show
                        when=move || {
                            discovered_models
                                .try_get()
                                .map(|models| !models.is_empty())
                                .unwrap_or(false)
                        }
                        fallback=move || view! {
                            <p class="wizard-step-hint">"No base model folders found yet."</p>
                        }
                    >
                        <div class="wizard-model-list">
                            {move || {
                                let selected_paths = selected_model_paths.try_get().unwrap_or_default();
                                discovered_models
                                    .try_get()
                                    .unwrap_or_default()
                                    .into_iter()
                                    .map(|model| {
                                        let path = model.path;
                                        let is_selected = selected_paths.iter().any(|p| p == &path);
                                        let action_label = if is_selected { "Deselect" } else { "Select" };
                                        let already_registered = model.already_registered;
                                        view! {
                                            <div class="wizard-model-item">
                                                <div>
                                                    <p class="text-sm font-semibold">{model.name}</p>
                                                    <p class="text-xs text-muted-foreground">{path.clone()}</p>
                                                    <p class="text-xs text-muted-foreground">{format!("{} / {}", model.format, model.backend)}</p>
                                                    {already_registered.then(|| view! {
                                                        <p class="text-xs text-muted-foreground">"Already in registry"</p>
                                                    })}
                                                </div>
                                                <Button
                                                    variant=ButtonVariant::Outline
                                                    size=ButtonSize::Sm
                                                    on_click=Callback::new(move |_| on_toggle_model.run(path.clone()))
                                                >
                                                    {action_label}
                                                </Button>
                                            </div>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </div>
                    </Show>
                    <ButtonLink
                        href="/models"
                        variant=ButtonVariant::Outline
                        size=ButtonSize::Sm
                        class="mt-2".to_string()
                    >
                        "Open Base Model Registry"
                    </ButtonLink>
                }
            >
                <div class="wizard-step-success">
                    <svg class="wizard-success-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/>
                        <polyline points="22 4 12 14.01 9 11.01"/>
                    </svg>
                    <span>{format!("{} base model(s) ready", model_count)}</span>
                </div>
            </Show>
        </div>
    }
}

#[component]
fn ReadyStep() -> impl IntoView {
    view! {
        <div class="wizard-action-area">
            <h3 class="wizard-step-title">"You Are Ready"</h3>
            <div class="wizard-step-success">
                <svg class="wizard-success-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/>
                    <polyline points="22 4 12 14.01 9 11.01"/>
                </svg>
                <span>"AdapterOS is ready for reliable conversations"</span>
            </div>
            <p class="wizard-step-desc">
                "Teach skills, run conversations, replay exact outputs, and review signed logs from one place."
            </p>
            <div class="wizard-ready-actions">
                <ButtonLink
                    href="/training?open_wizard=1"
                    variant=ButtonVariant::Primary
                    size=ButtonSize::Md
                >
                    "Create your first adapter"
                </ButtonLink>
                <ButtonLink
                    href="/chat"
                    variant=ButtonVariant::Outline
                    size=ButtonSize::Md
                >
                    "Open Chat"
                </ButtonLink>
                <ButtonLink
                    href="/"
                    variant=ButtonVariant::Ghost
                    size=ButtonSize::Md
                >
                    "Open Home"
                </ButtonLink>
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Main Welcome component
// ---------------------------------------------------------------------------

/// Welcome page for first-run setup guidance.
#[component]
pub fn Welcome() -> impl IntoView {
    let (status, refetch) = use_system_status();

    // SSE-driven refresh from Shell's health lifecycle stream.
    let health_refetch_counter = use_refetch_signal(RefetchTopic::Health);
    Effect::new(move || {
        let Some(counter) = health_refetch_counter.try_get() else {
            return;
        };
        if counter > 0 {
            refetch.run(());
        }
    });

    let on_refresh = Callback::new(move |_| refetch.run(()));

    // Wizard action state
    let (migrating, set_migrating) = signal(false);
    let (migrate_error, set_migrate_error) = signal(Option::<String>::None);
    let (discovering_models, set_discovering_models) = signal(false);
    let (seeding_models, set_seeding_models) = signal(false);
    let (discover_error, set_discover_error) = signal(Option::<String>::None);
    let (seed_error, set_seed_error) = signal(Option::<String>::None);
    let (seed_message, set_seed_message) = signal(Option::<String>::None);
    let (discovered_models, _set_discovered_models) = signal(Vec::<SetupDiscoveredModel>::new());
    let (selected_model_paths, set_selected_model_paths) = signal(Vec::<String>::new());

    // Capture the API client in the component's reactive scope
    #[cfg(target_arch = "wasm32")]
    let client = crate::api::use_api_client();
    #[cfg(target_arch = "wasm32")]
    let client_for_migrate = Arc::clone(&client);
    #[cfg(target_arch = "wasm32")]
    let client_for_discover = Arc::clone(&client);
    #[cfg(target_arch = "wasm32")]
    let client_for_seed = Arc::clone(&client);

    #[cfg(target_arch = "wasm32")]
    let refetch_for_migrate = refetch.clone();
    let on_migrate = Callback::new(move |()| {
        set_migrating.set(true);
        set_migrate_error.set(None);
        #[cfg(target_arch = "wasm32")]
        {
            let refetch = refetch_for_migrate.clone();
            let client = Arc::clone(&client_for_migrate);
            wasm_bindgen_futures::spawn_local(async move {
                match client.setup_migrate().await {
                    Ok(_) => {
                        set_migrating.set(false);
                        refetch.run(());
                    }
                    Err(e) => {
                        set_migrating.set(false);
                        set_migrate_error.set(Some(format!("Migration failed: {}", e)));
                    }
                }
            });
        }
    });

    let on_toggle_model = Callback::new(move |model_path: String| {
        set_selected_model_paths.update(|paths| {
            if let Some(index) = paths.iter().position(|p| p == &model_path) {
                paths.remove(index);
            } else {
                paths.push(model_path);
            }
        });
    });

    let on_discover = Callback::new(move |()| {
        set_discovering_models.set(true);
        set_discover_error.set(None);
        set_seed_error.set(None);
        set_seed_message.set(None);
        #[cfg(target_arch = "wasm32")]
        {
            let client = Arc::clone(&client_for_discover);
            wasm_bindgen_futures::spawn_local(async move {
                match client.setup_discover_models().await {
                    Ok(response) => {
                        let selected = response
                            .models
                            .iter()
                            .filter(|m| !m.already_registered)
                            .map(|m| m.path.clone())
                            .collect::<Vec<_>>();
                        set_discovering_models.set(false);
                        _set_discovered_models.set(response.models);
                        set_selected_model_paths.set(selected);
                    }
                    Err(e) => {
                        set_discovering_models.set(false);
                        set_discover_error.set(Some(format!("Discovery failed: {}", e)));
                    }
                }
            });
        }
    });

    #[cfg(target_arch = "wasm32")]
    let refetch_for_seed = refetch.clone();
    let on_seed_selected = Callback::new(move |()| {
        let paths = selected_model_paths.get_untracked();
        if paths.is_empty() {
            set_seed_error.set(Some("Select at least one model to seed.".to_string()));
            return;
        }

        set_seeding_models.set(true);
        set_seed_error.set(None);
        set_seed_message.set(None);
        #[cfg(target_arch = "wasm32")]
        {
            let client = Arc::clone(&client_for_seed);
            let refetch = refetch_for_seed.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client
                    .setup_seed_models(&SetupSeedModelsRequest {
                        model_paths: paths,
                        force: false,
                    })
                    .await
                {
                    Ok(response) => {
                        set_seeding_models.set(false);
                        set_seed_message.set(Some(format!(
                            "Seeded {}, skipped {}, failed {}.",
                            response.seeded_count, response.skipped_count, response.failed_count
                        )));
                        refetch.run(());
                    }
                    Err(e) => {
                        set_seeding_models.set(false);
                        set_seed_error.set(Some(format!("Seed failed: {}", e)));
                    }
                }
            });
        }
    });

    // Polling fallback when SSE events are unavailable.
    let _ = use_polling(10_000, move || async move {
        refetch.run(());
    });

    view! {
        <PageScaffold
            title="Welcome Home"
            subtitle="Guided setup for a safe first conversation"
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
                            "Follow these steps to bring the system online with reproducibility and signed audit coverage."
                        </p>
                    </div>

                    {move || {
                        match status.try_get().unwrap_or(LoadingState::Idle) {
                            LoadingState::Idle | LoadingState::Loading => view! {
                                <div class="welcome-loading">
                                    <Spinner />
                                    <span class="text-sm text-muted-foreground">"Checking system readiness\u{2026}"</span>
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
                                            <p class="text-sm font-semibold">"Could not reach AdapterOS"</p>
                                            <p class="text-xs text-muted-foreground">
                                                "Make sure the control plane is running: " <code>"./start"</code>
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
                                let current_step = wizard_step_from_status(s);
                                let db_ok = s.readiness.checks.db.status == StatusIndicator::Ready
                                    && s.readiness.checks.migrations.status == StatusIndicator::Ready;
                                let workers_ok = s.readiness.checks.workers.status == StatusIndicator::Ready;
                                let models_ok = s.readiness.checks.models.status == StatusIndicator::Ready;
                                let model_count = s.kernel.as_ref()
                                    .and_then(|k| k.models.as_ref())
                                    .and_then(|m| m.total)
                                    .unwrap_or(0);

                                view! {
                                    <div class="welcome-checklist">
                                        <WizardProgress current_step=current_step />

                                        <div class="wizard-step-content">
                                            <Show
                                                when=move || current_step == WizardStep::Database
                                                fallback=move || view! {}.into_any()
                                            >
                                                <DatabaseStep
                                                    db_ok=db_ok
                                                    on_migrate=on_migrate
                                                    migrating=migrating
                                                    migrate_error=migrate_error
                                                />
                                            </Show>
                                            <Show
                                                when=move || current_step == WizardStep::Worker
                                                fallback=move || view! {}.into_any()
                                            >
                                                <WorkerStep worker_connected=workers_ok />
                                            </Show>
                                            <Show
                                                when=move || current_step == WizardStep::Models
                                                fallback=move || view! {}.into_any()
                                            >
                                                <ModelsStep
                                                    models_seeded=models_ok
                                                    model_count=model_count
                                                    discovered_models=discovered_models
                                                    selected_model_paths=selected_model_paths
                                                    discovering=discovering_models
                                                    seeding=seeding_models
                                                    discover_error=discover_error
                                                    seed_error=seed_error
                                                    seed_message=seed_message
                                                    on_discover=on_discover
                                                    on_toggle_model=on_toggle_model
                                                    on_seed_selected=on_seed_selected
                                                />
                                            </Show>
                                            <Show
                                                when=move || current_step == WizardStep::Ready
                                                fallback=move || view! {}.into_any()
                                            >
                                                <ReadyStep />
                                            </Show>
                                        </div>

                                        // Compact checklist summary below the wizard
                                        {
                                            let checklist = derive_checklist(s);
                                            let ready_count = checklist.iter().filter(|c| c.status == CheckStatus::Ready).count();
                                            let total = checklist.len();
                                            view! {
                                                <div class="wizard-summary">
                                                    <div class="welcome-progress-bar">
                                                        <div class="welcome-progress-fill" style=format!("width: {}%", ready_count * 100 / total) />
                                                    </div>
                                                    <p class="welcome-progress-label">
                                                        {format!("{} of {} system checks complete", ready_count, total)}
                                                    </p>
                                                </div>
                                            }
                                        }
                                    </div>
                                }.into_any()
                            },
                        }
                    }}

                    <div class="welcome-skip">
                        <a href="/" class="welcome-skip-link">
                            "Go to Home"
                        </a>
                    </div>
                </div>
            </div>
        </PageScaffold>
    }
}
