//! LogicalControlRail - Persistent logical contract for runtime state.
//!
//! Makes active state, rules, transitions, and next actions explicit across all
//! routes. This rail is rendered directly in the shell under the top bar.

use super::topbar::{configuration_fingerprint, is_reproducible_mode_ready, short_fingerprint};
use crate::components::inference_guidance::{guidance_for, primary_blocker};
use crate::components::{Badge, BadgeVariant, CopyableId};
use crate::constants::ui_language;
use crate::hooks::{use_startup_health, use_system_status, LoadingState, StartupHealthResponse};
use adapteros_api_types::{InferenceBlocker, InferenceReadyState, SystemStatusResponse};
use leptos::prelude::*;
use leptos_router::hooks::use_location;

#[derive(Debug, Clone, PartialEq)]
struct LogicSnapshot {
    fingerprint: String,
    base_model: String,
    stack_version: String,
    primary_blocker: Option<InferenceBlocker>,
    inference_ready: InferenceReadyState,
    reproducible_ready: bool,
}

impl LogicSnapshot {
    fn from_status(status: &SystemStatusResponse) -> Self {
        let base_model = status
            .kernel
            .as_ref()
            .and_then(|kernel| kernel.model.as_ref())
            .and_then(|model| model.model_id.clone())
            .unwrap_or_else(|| "No base model active".to_string());
        let stack_version = status
            .kernel
            .as_ref()
            .and_then(|kernel| kernel.plan.as_ref())
            .map(|plan| plan.plan_id.clone())
            .unwrap_or_else(|| "Stack version pending".to_string());

        Self {
            fingerprint: configuration_fingerprint(status),
            base_model,
            stack_version,
            primary_blocker: primary_blocker(&status.inference_blockers).cloned(),
            inference_ready: status.inference_ready,
            reproducible_ready: is_reproducible_mode_ready(status),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContractState {
    Enforced,
    Attention,
    Pending,
}

#[derive(Debug, Clone)]
struct LogicTransition {
    title: String,
    reason: String,
    consequence: String,
    state: ContractState,
}

#[component]
pub fn LogicalControlRail() -> impl IntoView {
    let location = use_location();
    let (system_status, refetch_status) = use_system_status();
    let (startup_health, refetch_startup_health) = use_startup_health();

    let is_home = move || {
        location
            .pathname
            .try_get()
            .map(|p| p == "/" || p == "/dashboard")
            .unwrap_or(false)
    };

    let previous_snapshot = RwSignal::new(None::<LogicSnapshot>);
    let latest_transition = RwSignal::new(None::<LogicTransition>);

    Effect::new(move || {
        let Some(LoadingState::Loaded(status)) = system_status.try_get() else {
            return;
        };
        let next = LogicSnapshot::from_status(&status);

        if let Some(previous) = previous_snapshot.get_untracked() {
            if let Some(transition) = derive_transition(&previous, &next) {
                latest_transition.set(Some(transition));
            }
        }

        previous_snapshot.set(Some(next));
    });

    let on_refresh = move |_| {
        refetch_status.run(());
        refetch_startup_health.run(());
    };

    view! {
        <section class="logic-rail" aria-label="System logic contract" aria-live="polite">
            {move || {
                if is_home() {
                    // Minimal variant on Home: fingerprint + Locked Output only
                    match system_status.get() {
                        LoadingState::Loaded(status) => {
                            let snapshot = LogicSnapshot::from_status(&status);
                            let lock_label = if snapshot.reproducible_ready {
                                "Locked Output active"
                            } else {
                                "Locked Output pending"
                            };
                            view! {
                                <div class="logic-rail__minimal">
                                    <div class="logic-rail__minimal-badges">
                                        <div class="logic-rail__minimal-badge" title="Current Configuration Fingerprint">
                                            <span class="logic-rail__minimal-label">{ui_language::CONFIG_FINGERPRINT_LABEL}</span>
                                            <span class="logic-rail__minimal-value">{short_fingerprint(&snapshot.fingerprint)}</span>
                                        </div>
                                        <div class="logic-rail__minimal-badge" title=lock_label>
                                            <span class="logic-rail__minimal-label">"Reproducible Mode"</span>
                                            <Badge variant=if snapshot.reproducible_ready {
                                                BadgeVariant::Success
                                            } else {
                                                BadgeVariant::Secondary
                                            }>
                                                {lock_label}
                                            </Badge>
                                        </div>
                                    </div>
                                    <a class="logic-link-btn logic-link-btn--primary" href="/system">
                                        "View full contract"
                                    </a>
                                </div>
                            }.into_any()
                        }
                        _ => view! {
                            <div class="logic-rail__minimal">
                                <div class="logic-rail__minimal-badges">
                                    <span class="text-muted-foreground text-sm">"Loading…"</span>
                                </div>
                            </div>
                        }.into_any(),
                    }
                } else {
                    view! {
                        <div class="logic-rail__header">
                            <div class="logic-rail__header-copy">
                                <p class="logic-rail__title">"System Logic Contract"</p>
                                <p class="logic-rail__subtitle">
                                    "The active state, safety boundaries, locks, and next consequences are always visible here."
                                </p>
                            </div>
                            <div class="logic-rail__header-actions">
                                <a class="logic-link-btn" href="/system">{ui_language::KERNEL_BOOT_SEQUENCE}</a>
                                <a class="logic-link-btn" href="/audit">{ui_language::EVENT_VIEWER}</a>
                                <button class="logic-link-btn" type="button" on:click=on_refresh>
                                    "Refresh contract"
                                </button>
                            </div>
                        </div>

                        {move || match system_status.get() {
                LoadingState::Loaded(status) => {
                    let snapshot = LogicSnapshot::from_status(&status);
                    let primary_blocker = snapshot.primary_blocker.as_ref();
                    let guidance = guidance_for(status.inference_ready, primary_blocker);
                    let adapters_active = status
                        .kernel
                        .as_ref()
                        .and_then(|kernel| kernel.adapters.as_ref())
                        .and_then(|adapters| adapters.total_active)
                        .map(|count| {
                            if count == 0 {
                                "No active adapters".to_string()
                            } else {
                                count.to_string()
                            }
                        })
                        .unwrap_or_else(|| "No active adapters".to_string());
                    let model_inventory = status
                        .kernel
                        .as_ref()
                        .and_then(|kernel| kernel.models.as_ref())
                        .map(|models| {
                            let loaded = models.loaded.unwrap_or_default();
                            let total = models.total.unwrap_or_default();
                            format!("{loaded}/{total} registered")
                        })
                        .unwrap_or_else(|| "Unknown".to_string());
                    let blocker_list = if status.inference_blockers.is_empty() {
                        "No active blockers".to_string()
                    } else {
                        status
                            .inference_blockers
                            .iter()
                            .map(blocker_label)
                            .collect::<Vec<_>>()
                            .join(" • ")
                    };
                    let (boot_state, boot_label, boot_detail) = startup_contract(&startup_health.get());
                    let lock_state = if snapshot.reproducible_ready {
                        ContractState::Enforced
                    } else {
                        ContractState::Pending
                    };
                    let safety_state = if status.inference_blockers.is_empty() {
                        ContractState::Enforced
                    } else {
                        ContractState::Attention
                    };
                    let logging_state = if snapshot.reproducible_ready {
                        ContractState::Enforced
                    } else {
                        ContractState::Pending
                    };

                    view! {
                        <div class="logic-rail__grid">
                            <article class="logic-card logic-card--fingerprint">
                                <div class="logic-card__kicker">{ui_language::CONFIG_FINGERPRINT_LABEL}</div>
                                <div class="logic-fingerprint-value" title=snapshot.fingerprint.clone()>
                                    {short_fingerprint(&snapshot.fingerprint)}
                                </div>
                                <div class="logic-fingerprint-copy">
                                    <CopyableId
                                        id=snapshot.fingerprint.clone()
                                        label="Full fingerprint".to_string()
                                    />
                                </div>
                                <div class="logic-card__footer">
                                    <a class="logic-inline-link" href="/runs">"Open provenance and restore points"</a>
                                </div>
                            </article>

                            <article class="logic-card">
                                <h3 class="logic-card__title">"Active Runtime Stack"</h3>
                                <dl class="logic-kv">
                                    <div>
                                        <dt>"Base model"</dt>
                                        <dd title=snapshot.base_model.clone()>{snapshot.base_model.clone()}</dd>
                                    </div>
                                    <div>
                                        <dt>"Stack version"</dt>
                                        <dd class="logic-kv__copyable" title=snapshot.stack_version.clone()>
                                            <CopyableId
                                                id=snapshot.stack_version.clone()
                                                label="".to_string()
                                                truncate=24
                                            />
                                        </dd>
                                    </div>
                                    <div>
                                        <dt>"Active adapters"</dt>
                                        <dd>{adapters_active}</dd>
                                    </div>
                                    <div>
                                        <dt>"Model inventory"</dt>
                                        <dd>{model_inventory}</dd>
                                    </div>
                                </dl>
                            </article>

                            <article class="logic-card">
                                <h3 class="logic-card__title">"Rule Contracts"</h3>
                                <div class="logic-rule-list">
                                    <RuleContractRow
                                        title="Reproducible lock"
                                        state=lock_state
                                        detail=if snapshot.reproducible_ready {
                                            "Locked Output is active for this fingerprint.".to_string()
                                        } else {
                                            "Locked Output is not fully active yet.".to_string()
                                        }
                                    />
                                    <RuleContractRow
                                        title="Safety boundary"
                                        state=safety_state
                                        detail=if status.inference_blockers.is_empty() {
                                            "No boundary is blocking prompt execution.".to_string()
                                        } else {
                                            format!("Prompt execution paused by: {}.", blocker_list)
                                        }
                                    />
                                    <RuleContractRow
                                        title=ui_language::KERNEL_BOOT_SEQUENCE
                                        state=boot_state
                                        detail=format!("{boot_label}. {boot_detail}")
                                    />
                                    <RuleContractRow
                                        title=ui_language::SIGNED_SYSTEM_LOGS
                                        state=logging_state
                                        detail="Each restore point can be verified and exported as tamper-proof evidence."
                                            .to_string()
                                    />
                                </div>
                            </article>

                            <article class="logic-card">
                                <h3 class="logic-card__title">"Next Logical Step"</h3>
                                <p class="logic-next-step">{guidance.reason}</p>
                                <p class="logic-next-consequence">
                                    {next_consequence(&snapshot)}
                                </p>
                                <div class="logic-next-actions">
                                    <a class="logic-link-btn logic-link-btn--primary" href=guidance.action.href>
                                        {guidance.action.label}
                                    </a>
                                    <a class="logic-link-btn" href="/models">{ui_language::BASE_MODEL_REGISTRY}</a>
                                    <a class="logic-link-btn" href="/workers">{ui_language::INFERENCE_ENGINES}</a>
                                </div>
                            </article>
                        </div>

                        <div class="logic-rail__transition">
                            <h4 class="logic-transition__title">"Latest Transition"</h4>
                            {move || {
                                if let Some(transition) = latest_transition.get() {
                                    view! {
                                        <div class=format!(
                                            "logic-transition logic-transition--{}",
                                            contract_state_class(transition.state)
                                        )>
                                            <p class="logic-transition__heading">{transition.title}</p>
                                            <p class="logic-transition__reason">{transition.reason}</p>
                                            <p class="logic-transition__consequence">{transition.consequence}</p>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="logic-transition logic-transition--pending">
                                            <p class="logic-transition__heading">"No state transition yet in this session"</p>
                                            <p class="logic-transition__reason">
                                                "This area will immediately explain what changed and why when state transitions occur."
                                            </p>
                                            <p class="logic-transition__hint">
                                                "Change the base model, stack, or safety settings to see a transition."
                                            </p>
                                        </div>
                                    }.into_any()
                                }
                            }}
                        </div>

                        <div class="logic-rail__action-strip">
                            <a class="logic-action-pill" href="/chat">"Open Prompt Studio"</a>
                            <a class="logic-action-pill" href="/stacks">"Open Adapter Stack"</a>
                            <a class="logic-action-pill" href="/runs">{ui_language::SYSTEM_RESTORE_POINTS}</a>
                            <a class="logic-action-pill" href="/audit">{ui_language::EVENT_VIEWER}</a>
                            <a class="logic-action-pill" href="/policies">{ui_language::SAFETY_SHIELD}</a>
                            <a class="logic-action-pill" href="/settings">"Open Kernel Settings"</a>
                        </div>
                    }
                    .into_any()
                }
                LoadingState::Error(err) => view! {
                    <div class="logic-rail__loading logic-rail__loading--error">
                        <p class="font-medium">"System logic snapshot unavailable"</p>
                        <p class="text-xs text-muted-foreground">
                            {format!("Contract feed error: {}", err.user_message())}
                        </p>
                        <a class="logic-link-btn logic-link-btn--primary" href="/system">
                            "Open Kernel diagnostics"
                        </a>
                    </div>
                }
                .into_any(),
                LoadingState::Idle | LoadingState::Loading => view! {
                    <div class="logic-rail__loading">
                        <p class="font-medium">"Building system logic snapshot"</p>
                        <p class="text-xs text-muted-foreground">
                            "Preparing full state visibility for locks, rules, and next actions."
                        </p>
                    </div>
                }
                .into_any(),
            }}
                    }.into_any()
                }
            }}
        </section>
    }
}

#[component]
fn RuleContractRow(title: &'static str, state: ContractState, detail: String) -> impl IntoView {
    view! {
        <div class="logic-rule-row">
            <div class="logic-rule-row__header">
                <p class="logic-rule-row__title">{title}</p>
                <Badge variant=contract_badge_variant(state)>{contract_state_label(state)}</Badge>
            </div>
            <p class="logic-rule-row__detail">{detail}</p>
        </div>
    }
}

fn startup_contract(
    health: &LoadingState<StartupHealthResponse>,
) -> (ContractState, String, String) {
    match health {
        LoadingState::Loaded(boot) => {
            let status = boot.status.to_ascii_lowercase();
            if status == "ready" {
                (
                    ContractState::Enforced,
                    "Kernel ready".to_string(),
                    "Boot contract satisfied and inference path can be evaluated.".to_string(),
                )
            } else if status == "degraded" {
                (
                    ContractState::Attention,
                    "Kernel ready with safeguards".to_string(),
                    boot.next_action.clone(),
                )
            } else if status == "failed" {
                (
                    ContractState::Attention,
                    "Kernel boot failed".to_string(),
                    boot.next_action.clone(),
                )
            } else {
                (
                    ContractState::Pending,
                    "Kernel boot in progress".to_string(),
                    boot.next_action.clone(),
                )
            }
        }
        LoadingState::Error(_) => (
            ContractState::Attention,
            "Kernel boot status unavailable".to_string(),
            "Open Kernel diagnostics to inspect startup state.".to_string(),
        ),
        LoadingState::Idle | LoadingState::Loading => (
            ContractState::Pending,
            "Kernel boot status loading".to_string(),
            "Collecting boot sequence details.".to_string(),
        ),
    }
}

fn derive_transition(previous: &LogicSnapshot, next: &LogicSnapshot) -> Option<LogicTransition> {
    if previous.fingerprint != next.fingerprint {
        let reason = if previous.base_model != next.base_model {
            format!(
                "Base model changed from '{}' to '{}'.",
                previous.base_model, next.base_model
            )
        } else if previous.stack_version != next.stack_version {
            format!(
                "Stack version changed from '{}' to '{}'.",
                previous.stack_version, next.stack_version
            )
        } else {
            "Runtime lock conditions changed.".to_string()
        };

        return Some(LogicTransition {
            title: "Configuration fingerprint changed".to_string(),
            reason,
            consequence: if next.inference_ready == InferenceReadyState::True {
                "New prompts will execute under the new fingerprint and lock conditions."
                    .to_string()
            } else {
                "Prompt execution remains paused until active blockers clear.".to_string()
            },
            state: if next.inference_ready == InferenceReadyState::True {
                ContractState::Enforced
            } else {
                ContractState::Attention
            },
        });
    }

    if previous.inference_ready != next.inference_ready {
        return Some(LogicTransition {
            title: "Prompt execution state changed".to_string(),
            reason: format!(
                "Execution changed from '{}' to '{}'.",
                inference_state_label(previous.inference_ready),
                inference_state_label(next.inference_ready),
            ),
            consequence: next_consequence(next),
            state: if next.inference_ready == InferenceReadyState::True {
                ContractState::Enforced
            } else {
                ContractState::Attention
            },
        });
    }

    if previous.primary_blocker != next.primary_blocker {
        let reason = match (&previous.primary_blocker, &next.primary_blocker) {
            (Some(prev), Some(next)) => {
                format!(
                    "Primary boundary switched from '{}' to '{}'.",
                    blocker_label(prev),
                    blocker_label(next),
                )
            }
            (None, Some(next)) => {
                format!("A new boundary became active: '{}'.", blocker_label(next))
            }
            (Some(prev), None) => {
                format!("Boundary '{}' was cleared.", blocker_label(prev))
            }
            (None, None) => "No boundary changes.".to_string(),
        };
        return Some(LogicTransition {
            title: "Safety boundary changed".to_string(),
            reason,
            consequence: next_consequence(next),
            state: if next.primary_blocker.is_some() {
                ContractState::Attention
            } else {
                ContractState::Enforced
            },
        });
    }

    None
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

fn next_consequence(snapshot: &LogicSnapshot) -> String {
    match snapshot.inference_ready {
        InferenceReadyState::True => {
            "Consequence: prompts run immediately with reproducible tracking and signed logs."
                .to_string()
        }
        InferenceReadyState::False => match snapshot.primary_blocker.as_ref() {
            Some(blocker) => format!(
                "Consequence: prompt execution stays paused until '{}' is cleared.",
                blocker_label(blocker)
            ),
            None => {
                "Consequence: prompt execution is paused until all readiness checks recover."
                    .to_string()
            }
        },
        InferenceReadyState::Unknown => {
            "Consequence: readiness is still being evaluated; defer live changes until status resolves."
                .to_string()
        }
    }
}

fn inference_state_label(state: InferenceReadyState) -> &'static str {
    match state {
        InferenceReadyState::True => "ready",
        InferenceReadyState::False => "blocked",
        InferenceReadyState::Unknown => "unknown",
    }
}

fn contract_badge_variant(state: ContractState) -> BadgeVariant {
    match state {
        ContractState::Enforced => BadgeVariant::Success,
        ContractState::Attention => BadgeVariant::Warning,
        ContractState::Pending => BadgeVariant::Secondary,
    }
}

fn contract_state_label(state: ContractState) -> &'static str {
    match state {
        ContractState::Enforced => "Enforced",
        ContractState::Attention => "Attention",
        ContractState::Pending => "Pending",
    }
}

fn contract_state_class(state: ContractState) -> &'static str {
    match state {
        ContractState::Enforced => "enforced",
        ContractState::Attention => "attention",
        ContractState::Pending => "pending",
    }
}
