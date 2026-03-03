//! Adapter Details Panel Component
//!
//! Displays detailed adapter information in an expand/collapse drawer pattern.
//! Shows: adapter name, hash, last trained time, source dataset, tenant.
//! Provides Load/Unload controls when backend supports them.
//! Includes "why suggested" explanation string for routing context.
//!
//! ## Design
//!
//! Uses Liquid Glass Tier 2 (panels) with 12px blur and 78% alpha.
//! Deterministic content - no random elements, consistent ordering.

use adapteros_api_types::{
    AdapterResponse, LifecycleState, TrainingJobResponse, TrainingListParams,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_location, use_navigate};
use std::sync::Arc;
use urlencoding::encode;

use crate::api::{use_api_client, ApiError};
use crate::components::{
    AdapterLifecycleControls, Badge, BadgeVariant, Button, ButtonSize, ButtonType, ButtonVariant,
    Card, ConfirmationDialog, ConfirmationSeverity, CopyableId, DetailGrid, DetailItem, EmptyState,
    EmptyStateVariant, IconX, Input, ProvenanceBadge, Spinner,
};
use crate::contexts::use_in_flight;
use crate::signals::notifications::use_notifications;
use crate::utils::{chat_path_with_adapter, format_bytes, humanize, status_display_label};

/// Suggestion context explaining why an adapter was suggested.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AdapterSuggestionContext {
    /// Why this adapter was suggested (e.g., "Matches framework: React 18.2")
    pub reason: Option<String>,
    /// Confidence score from the router (0.0-1.0)
    pub confidence: Option<f32>,
    /// Gate value from K-sparse routing
    pub gate_value: Option<f64>,
    /// Whether this adapter is currently pinned
    pub is_pinned: bool,
}

/// Adapter detail panel component.
///
/// Displays comprehensive adapter information in a right-side drawer panel.
/// Follows the same pattern as DataDetailPanel for consistency.
#[component]
pub fn AdapterDetailPanel(
    /// The adapter data to display (None shows empty state)
    #[prop(into)]
    adapter: Signal<Option<AdapterResponse>>,
    /// Suggestion context for "why suggested" explanation
    #[prop(optional, into)]
    suggestion_context: Option<Signal<AdapterSuggestionContext>>,
    /// Whether data is loading
    #[prop(into, default = Signal::derive(|| false))]
    loading: Signal<bool>,
    /// Callback when close is requested
    on_close: Callback<()>,
    /// Callback invoked to refresh adapter data after lifecycle transitions
    #[prop(optional)]
    on_refetch: Option<Callback<()>>,
    /// Callback for pin/unpin action (adapter_id)
    #[prop(optional)]
    on_toggle_pin: Option<Callback<String>>,
    /// Callback for load action (adapter_id) - shown if backend supports
    #[prop(optional)]
    on_load: Option<Callback<String>>,
    /// Callback for unload action (adapter_id) - shown if backend supports
    #[prop(optional)]
    on_unload: Option<Callback<String>>,
) -> impl IntoView {
    view! {
        <div class="adapter-detail-panel">
            {move || {
                if loading.try_get().unwrap_or(false) {
                    return view! {
                        <div class="adapter-detail-loading">
                            <Spinner />
                        </div>
                    }.into_any();
                }

                match adapter.try_get().flatten() {
                    None => view! {
                        <AdapterDetailEmpty />
                    }.into_any(),
                    Some(data) => {
                        let ctx = suggestion_context
                            .and_then(|s| s.try_get())
                            .unwrap_or_default();
                        view! {
                            <AdapterDetailContent
                                adapter=data
                                suggestion_context=ctx
                                on_close=on_close
                                on_refetch=on_refetch
                                on_toggle_pin=on_toggle_pin
                                on_load=on_load
                                on_unload=on_unload
                            />
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Empty state when no adapter is selected.
#[component]
fn AdapterDetailEmpty() -> impl IntoView {
    view! {
        <EmptyState
            variant=EmptyStateVariant::Empty
            title="No skill selected".to_string()
            description="Select a skill to view details, trust context, and update controls.".to_string()
        />
    }
}

fn rename_error_message(action: &str, error: &ApiError) -> String {
    if matches!(error, ApiError::Forbidden(_)) || error.code() == Some("FORBIDDEN") {
        format!(
            "You do not have permission to {}. Ask an administrator for adapter-management access.",
            action
        )
    } else {
        format!("Unable to {}: {}", action, error.user_message())
    }
}

/// Adapter detail content view.
#[component]
fn AdapterDetailContent(
    adapter: AdapterResponse,
    suggestion_context: AdapterSuggestionContext,
    on_close: Callback<()>,
    on_refetch: Option<Callback<()>>,
    on_toggle_pin: Option<Callback<String>>,
    on_load: Option<Callback<String>>,
    on_unload: Option<Callback<String>>,
) -> impl IntoView {
    // Access in-flight context
    let in_flight = use_in_flight();
    let client = use_api_client();

    // Rename state
    let name_editing = RwSignal::new(false);
    let name_draft = RwSignal::new(String::new());
    let renaming = RwSignal::new(false);
    let action_error = RwSignal::new(None::<String>);
    let action_success = RwSignal::new(None::<String>);

    // Clone values needed for closures
    let adapter_id_for_pin = adapter.adapter_id.clone();
    let adapter_id_for_load = adapter.adapter_id.clone();
    let adapter_id_for_unload = adapter.adapter_id.clone();
    let adapter_id_for_flight = adapter.adapter_id.clone();
    let adapter_id_for_lifecycle = adapter.adapter_id.clone();
    let adapter_name_for_lifecycle = adapter.name.clone();
    let lifecycle_state_for_controls = adapter.lifecycle_state.to_string();

    // Derive lifecycle badge variant
    let lifecycle_variant = match adapter.lifecycle_state {
        LifecycleState::Active => BadgeVariant::Success,
        LifecycleState::Staging => BadgeVariant::Warning,
        LifecycleState::Deprecated => BadgeVariant::Warning,
        LifecycleState::Retired => BadgeVariant::Destructive,
        LifecycleState::Draft => BadgeVariant::Secondary,
        _ => BadgeVariant::Default,
    };

    // Derive runtime state badge variant
    let runtime_variant = match adapter.runtime_state.as_deref() {
        Some("hot") => BadgeVariant::Success,
        Some("warm") => BadgeVariant::Warning,
        Some("cold") => BadgeVariant::Secondary,
        Some("resident") => BadgeVariant::Success,
        Some("unloaded") => BadgeVariant::Default,
        _ => BadgeVariant::Default,
    };

    // Determine if load/unload buttons should be shown
    let can_load = adapter
        .runtime_state
        .as_deref()
        .map(|s| s == "unloaded" || s == "cold")
        .unwrap_or(true);
    let can_unload = adapter
        .runtime_state
        .as_deref()
        .map(|s| s == "hot" || s == "warm" || s == "resident")
        .unwrap_or(false);

    // Extract display values
    let name = adapter.name.clone();
    let adapter_id = adapter.adapter_id.clone();
    let hash_b3 = adapter.hash_b3.clone();
    let tier = adapter.tier.clone();
    let category = adapter.category.clone().unwrap_or_else(|| "N/A".into());
    let scope = adapter.scope.clone().unwrap_or_else(|| "N/A".into());
    let lifecycle_state = lifecycle_stage_label(adapter.lifecycle_state).to_string();
    let runtime_state = runtime_state_label(adapter.runtime_state.as_deref()).to_string();
    let created_at = adapter.created_at.clone();
    let updated_at = adapter.updated_at.clone();
    let version = adapter.version.clone();
    let rank = adapter.rank;
    let languages = adapter.languages.clone();
    let framework = adapter.framework.clone();
    let intent = adapter.intent.clone();
    let memory_bytes = adapter.memory_bytes;
    let is_pinned = adapter.pinned.unwrap_or(false);
    let repo_id = adapter.repo_id.clone();
    let repo_id_for_versions = repo_id.clone();
    let adapter_stats_for_relationships = adapter.stats.clone();
    let adapter_stats_for_overview = adapter.stats.clone();
    let adapter_chat_path = chat_path_with_adapter(&adapter_id);
    let compatibility_text = describe_adapter_compatibility(framework.as_deref(), &languages);
    let readiness_text =
        lifecycle_readiness_text(adapter.lifecycle_state, adapter.runtime_state.as_deref());
    let relationship_version = version.clone();

    // Related training context for relationship surfaces.
    let related_training_job = RwSignal::new(None::<TrainingJobResponse>);
    let related_training_loading = RwSignal::new(true);
    let related_training_error = RwSignal::new(None::<String>);
    {
        let client = Arc::clone(&client);
        let adapter_name_for_lookup = name.clone();
        let adapter_id_for_lookup = adapter_id.clone();
        spawn_local(async move {
            let params = TrainingListParams {
                page: Some(1),
                page_size: Some(100),
                adapter_name: Some(adapter_name_for_lookup.clone()),
                ..Default::default()
            };
            match client.list_training_jobs(Some(&params)).await {
                Ok(resp) => {
                    let latest = resp
                        .jobs
                        .into_iter()
                        .filter(|job| {
                            job.adapter_id.as_deref() == Some(adapter_id_for_lookup.as_str())
                                || job.adapter_name == adapter_name_for_lookup
                        })
                        .max_by(|left, right| left.created_at.cmp(&right.created_at));
                    related_training_job.set(latest);
                    related_training_error.set(None);
                }
                Err(err) => {
                    related_training_job.set(None);
                    related_training_error.set(Some(err.user_message()));
                }
            }
            related_training_loading.set(false);
        });
    }

    // Suggestion context values
    let has_suggestion = suggestion_context.reason.is_some()
        || suggestion_context.confidence.is_some()
        || suggestion_context.gate_value.is_some();
    let suggestion_reason = suggestion_context
        .reason
        .clone()
        .unwrap_or_else(|| "Best match for this request".into());
    let suggestion_confidence = suggestion_context.confidence;
    let suggestion_gate = suggestion_context.gate_value;
    let suggestion_pinned = suggestion_context.is_pinned;

    // Derive reactive in-flight status
    let is_in_flight = Signal::derive(move || in_flight.is_in_flight(&adapter_id_for_flight));

    // Rename allowed only for Draft and Training (backend PolicyViolation otherwise)
    let can_rename = matches!(
        adapter.lifecycle_state,
        LifecycleState::Draft | LifecycleState::Training
    );
    let rename_aria_label = if can_rename {
        "Rename adapter".to_string()
    } else {
        format!(
            "Rename not available for adapters in {} state",
            lifecycle_state.clone()
        )
    };

    // Rename callbacks
    let name_for_start = name.clone();
    let start_rename = Callback::new(move |_| {
        name_draft.set(name_for_start.clone());
        name_editing.set(true);
    });

    let cancel_rename = Callback::new(move |_| {
        name_editing.set(false);
        name_draft.set(String::new());
    });

    let save_rename = Callback::new({
        let client = client.clone();
        let adapter_id_for_rename = adapter_id.clone();
        move |_| {
            if renaming.get() {
                return;
            }
            let new_name = name_draft.get().trim().to_string();
            if new_name.is_empty() {
                action_error.set(Some("Name cannot be empty.".to_string()));
                return;
            }
            renaming.set(true);
            action_error.set(None);
            action_success.set(None);

            let client = client.clone();
            let adapter_id = adapter_id_for_rename.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client
                    .patch_adapter(&adapter_id, Some(new_name.trim()))
                    .await
                {
                    Ok(_) => {
                        name_editing.set(false);
                        name_draft.set(String::new());
                        action_success.set(Some("Adapter renamed.".to_string()));
                        if let Some(ref cb) = on_refetch {
                            cb.run(());
                        }
                    }
                    Err(e) => {
                        action_error.set(Some(rename_error_message("rename this adapter", &e)));
                    }
                }
                renaming.set(false);
            });
        }
    });

    let clear_alias = Callback::new({
        let client = client.clone();
        let adapter_id_for_clear = adapter_id.clone();
        move |_| {
            if renaming.get() {
                return;
            }
            renaming.set(true);
            action_error.set(None);
            action_success.set(None);

            let client = client.clone();
            let adapter_id = adapter_id_for_clear.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client.patch_adapter(&adapter_id, None).await {
                    Ok(_) => {
                        name_editing.set(false);
                        name_draft.set(String::new());
                        action_success.set(Some("Custom name cleared; using default.".to_string()));
                        if let Some(ref cb) = on_refetch {
                            cb.run(());
                        }
                    }
                    Err(e) => {
                        action_error.set(Some(rename_error_message("clear custom name", &e)));
                    }
                }
                renaming.set(false);
            });
        }
    });

    view! {
        <div class="adapter-detail-content">
            // Action feedback
            {move || action_error.get().map(|message| view! {
                <div class="mb-3 rounded-lg border border-destructive/50 bg-destructive/10 p-3">
                    <p class="text-sm text-destructive">{message}</p>
                </div>
            })}
            {move || action_success.get().map(|message| view! {
                <div class="mb-3 rounded-lg border border-status-success/50 bg-status-success/5 p-3">
                    <p class="text-sm text-status-success">{message}</p>
                </div>
            })}

            // Header with close button
            <div class="adapter-detail-header">
                <div class="adapter-detail-header-info">
                    {move || if name_editing.get() {
                        view! {
                            <div class="flex items-center gap-2" role="group" aria-label="Edit adapter name">
                                <Input
                                    value=name_draft
                                    placeholder="Adapter name".to_string()
                                    class="flex-1".to_string()
                                />
                                <Button
                                    button_type=ButtonType::Button
                                    variant=ButtonVariant::Primary
                                    disabled=Signal::derive(move || renaming.get())
                                    loading=Signal::derive(move || renaming.get())
                                    on_click=save_rename
                                    aria_label="Save adapter name"
                                >
                                    "Save"
                                </Button>
                                <Button
                                    button_type=ButtonType::Button
                                    variant=ButtonVariant::Ghost
                                    disabled=Signal::derive(move || renaming.get())
                                    on_click=clear_alias
                                    aria_label="Clear custom name and use default"
                                >
                                    "Use default"
                                </Button>
                                <Button
                                    button_type=ButtonType::Button
                                    variant=ButtonVariant::Ghost
                                    disabled=Signal::derive(move || renaming.get())
                                    on_click=cancel_rename
                                    aria_label="Cancel editing"
                                >
                                    "Cancel"
                                </Button>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="flex items-center gap-2">
                                <h2 class="adapter-detail-title flex-1">{name.clone()}</h2>
                                <Button
                                    button_type=ButtonType::Button
                                    variant=ButtonVariant::Ghost
                                    disabled=Signal::derive(move || !can_rename)
                                    on_click=start_rename
                                    aria_label=rename_aria_label.clone()
                                >
                                    "Rename"
                                </Button>
                            </div>
                        }.into_any()
                    }}
                    <p class="text-xs text-muted-foreground">
                        "IDs and hashes are in Technical Details below."
                    </p>
                </div>
                <button
                    type="button"
                    class="adapter-detail-close"
                    on:click=move |_| on_close.run(())
                    aria-label="Close detail panel"
                >
                    <IconX class="w-5 h-5"/>
                </button>
            </div>

            // Status badges
            <div class="adapter-detail-status">
                <Badge variant=lifecycle_variant>{lifecycle_state}</Badge>
                <Badge variant=runtime_variant>{runtime_state}</Badge>
                {move || is_in_flight.try_get().unwrap_or(false).then(|| view! {
                    <Badge variant=BadgeVariant::Warning>"In Use"</Badge>
                })}
                {is_pinned.then(|| view! {
                    <Badge variant=BadgeVariant::Secondary>
                        <span class="flex items-center gap-1">
                            <svg class="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                                <path d="M5 5a2 2 0 012-2h6a2 2 0 012 2v2h2a1 1 0 011 1v1a1 1 0 01-1 1h-1v5a3 3 0 01-3 3H7a3 3 0 01-3-3v-5H3a1 1 0 01-1-1V8a1 1 0 011-1h2V5z"/>
                            </svg>
                            "Pinned"
                        </span>
                    </Badge>
                })}
                <ProvenanceBadge />
            </div>

            // Why Suggested section (if context provided)
            {has_suggestion.then(move || {
                let confidence_pct = suggestion_confidence.map(|c| (c * 100.0) as u32);
                let gate_display = suggestion_gate.map(|g| format!("{:.4}", g));

                view! {
                    <Card title="Why Suggested">
                        <div class="adapter-detail-suggestion">
                            <p class="adapter-detail-suggestion-reason">{suggestion_reason.clone()}</p>
                            <div class="adapter-detail-suggestion-metrics">
                                {confidence_pct.map(|pct| view! {
                                    <div class="adapter-detail-metric">
                                        <span class="adapter-detail-metric-label">"Confidence"</span>
                                        <span class="adapter-detail-metric-value">{format!("{}%", pct)}</span>
                                    </div>
                                })}
                                {gate_display.map(|gate| view! {
                                    <div class="adapter-detail-metric">
                                        <span class="adapter-detail-metric-label">"Match Score"</span>
                                        <span class="adapter-detail-metric-value font-mono">{gate}</span>
                                    </div>
                                })}
                                {suggestion_pinned.then(|| view! {
                                    <div class="adapter-detail-metric">
                                        <span class="adapter-detail-metric-label">"Status"</span>
                                        <span class="adapter-detail-metric-value">"Pinned by user"</span>
                                    </div>
                                })}
                            </div>
                        </div>
                    </Card>
                }
            })}

            <Card title="Relationships">
                <div class="space-y-3">
                    <div>
                        <p class="text-sm">{compatibility_text.clone()}</p>
                        <p class="text-xs text-muted-foreground">{readiness_text.clone()}</p>
                    </div>
                    <div class="grid gap-3 sm:grid-cols-2">
                        <div class="rounded-md border border-border/50 bg-muted/20 p-3">
                            <p class="text-xs text-muted-foreground">"Adapter Version"</p>
                            <p class="text-sm font-medium">{relationship_version.clone()}</p>
                        </div>
                        <div class="rounded-md border border-border/50 bg-muted/20 p-3">
                            <p class="text-xs text-muted-foreground">"Base Model"</p>
                            <p class="text-sm font-medium">
                                {move || {
                                    related_training_job
                                        .try_get()
                                        .flatten()
                                        .and_then(|job| job.base_model_id)
                                        .unwrap_or_else(|| "Not exposed in current adapter relationship data".to_string())
                                }}
                            </p>
                        </div>
                    </div>

                    {move || {
                        if related_training_loading.try_get().unwrap_or(false) {
                            return view! {
                                <p class="text-xs text-muted-foreground">
                                    "Loading relationship links from recent training jobs..."
                                </p>
                            }.into_any();
                        }

                        if let Some(err) = related_training_error.try_get().flatten() {
                            return view! {
                                <p class="text-xs text-muted-foreground">
                                    {format!("Training relationship lookup unavailable: {}", err)}
                                </p>
                            }.into_any();
                        }

                        if let Some(job) = related_training_job.try_get().flatten() {
                            let dataset_refs = job
                                .dataset_version_trust
                                .clone()
                                .unwrap_or_default()
                                .into_iter()
                                .map(|entry| {
                                    let dataset_version_id = entry.dataset_version_id.clone();
                                    let label = entry
                                        .dataset_name
                                        .clone()
                                        .unwrap_or_else(|| format!("Dataset version {}", dataset_version_id));
                                    (entry.dataset_id, dataset_version_id, label)
                                })
                                .collect::<Vec<_>>();
                            let dataset_ref_count = dataset_refs.len();
                            let source_documents_text = if let Some(collection_id) = job.collection_id.clone() {
                                format!(
                                    "Document collection {} was used. Individual source documents are not listed in this payload.",
                                    collection_id
                                )
                            } else {
                                "Source documents are not exposed by current adapter relationship data.".to_string()
                            };
                            let report_href = format!("/v1/training/jobs/{}/report", job.id);
                            let job_href = format!("/training/{}", job.id);
                            view! {
                                <div class="space-y-3">
                                    <div>
                                        <p class="text-xs text-muted-foreground">"Upstream Dataset Versions"</p>
                                        {if dataset_refs.is_empty() {
                                            view! {
                                                <p class="text-sm">
                                                    "No dataset version links were attached to the latest related training job."
                                                </p>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <div class="space-y-1">
                                                    {dataset_refs.into_iter().map(|(dataset_id, dataset_version_id, label)| {
                                                        view! {
                                                            <div class="flex flex-wrap items-center gap-2 text-sm">
                                                                {if let Some(did) = dataset_id {
                                                                    view! {
                                                                        <a href=format!("/datasets/{}", did) class="text-primary hover:underline">{label.clone()}</a>
                                                                    }.into_any()
                                                                } else {
                                                                    view! {
                                                                        <span>{label}</span>
                                                                    }.into_any()
                                                                }}
                                                                <span class="text-xs text-muted-foreground font-mono">{dataset_version_id}</span>
                                                            </div>
                                                        }
                                                    }).collect_view()}
                                                    <p class="text-xs text-muted-foreground">
                                                        {format!("Provenance count: {} linked dataset versions.", dataset_ref_count)}
                                                    </p>
                                                </div>
                                            }.into_any()
                                        }}
                                    </div>
                                    <div>
                                        <p class="text-xs text-muted-foreground">"Source Documents"</p>
                                        <p class="text-sm">{source_documents_text}</p>
                                    </div>
                                    <div>
                                        <p class="text-xs text-muted-foreground">"Related Training and Report"</p>
                                        <div class="flex flex-wrap items-center gap-3 text-sm">
                                            <a href=job_href.clone() class="text-primary hover:underline">
                                                {format!("Open training job {}", job.id)}
                                            </a>
                                            <a href=report_href class="text-primary hover:underline">
                                                "Open report artifact"
                                            </a>
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="space-y-2">
                                    <p class="text-xs text-muted-foreground">"Upstream Dataset Versions"</p>
                                    <p class="text-sm">
                                        "No related training job was found for this adapter in the current list response."
                                    </p>
                                    <p class="text-xs text-muted-foreground">
                                        "Training/report links and source-document relationships are partial until that linkage is present."
                                    </p>
                                </div>
                            }.into_any()
                        }
                    }}

                    <div class="rounded-md border border-border/50 bg-muted/20 p-3">
                        <p class="text-xs text-muted-foreground">"Chats Used"</p>
                        <p class="text-sm">
                            {adapter_stats_for_relationships.clone().map(|stats| {
                                format!(
                                    "{} routed selections recorded. Individual chat sessions are not exposed on this adapter endpoint.",
                                    stats.selected_count
                                )
                            }).unwrap_or_else(|| "Usage counts are not available for this adapter yet.".to_string())}
                        </p>
                        <a href=adapter_chat_path.clone() class="text-xs text-primary hover:underline">
                            "Start a chat with this adapter"
                        </a>
                    </div>
                </div>
            </Card>

            // Overview grid section
            <div class="mt-6 mb-4">
                <h4 class="text-sm font-medium mb-3">"Overview"</h4>
                <DetailGrid class="bg-muted/30 p-4 rounded-md border text-sm text-foreground">
                    {adapter_stats_for_overview.clone().map(|stats| view! {
                        <DetailItem label="Activations" value=stats.total_activations.to_string() />
                    })}
                    <DetailItem label="Adapter Version" value=version.clone() mono=true />
                    <DetailItem label="Tier" value=tier />
                    <DetailItem label="Category" value=category />
                    <DetailItem label="Coverage" value=scope />
                    <DetailItem label="Capacity" value=rank.to_string() />
                    {memory_bytes.map(|bytes| view! {
                        <DetailItem label="Memory Usage" value=format_bytes(bytes) />
                    })}
                </DetailGrid>
            </div>

            // Languages & Framework
            {(!languages.is_empty() || framework.is_some()).then(move || {
                let langs = languages.clone();
                let fw = framework.clone();
                view! {
                    <Card title="Capabilities">
                        <div class="adapter-detail-tags">
                            {if langs.is_empty() {
                                view! {
                                    <span class="text-muted-foreground text-sm">"No languages specified"</span>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="flex flex-wrap gap-2">
                                        {langs.into_iter().map(|lang| view! {
                                            <Badge variant=BadgeVariant::Secondary>{lang}</Badge>
                                        }).collect::<Vec<_>>()}
                                    </div>
                                }.into_any()
                            }}
                            {fw.map(|framework_name| view! {
                                <div class="adapter-detail-framework">
                                    <span class="text-sm text-muted-foreground">"Primary Framework: "</span>
                                    <span class="font-medium">{framework_name}</span>
                                </div>
                            })}
                        </div>
                    </Card>
                }
            })}

            // Intent (if available)
            {intent.map(|intent_text| view! {
                <Card title="Intent">
                    <p class="text-sm">{intent_text}</p>
                </Card>
            })}

            // Timestamps
            <Card title="History">
                <dl class="adapter-detail-metadata">
                    <div class="adapter-detail-metadata-item">
                        <dt>"Created"</dt>
                        <dd>{created_at}</dd>
                    </div>
                    {updated_at.map(|updated| view! {
                        <div class="adapter-detail-metadata-item">
                            <dt>"Last Updated"</dt>
                            <dd>{updated}</dd>
                        </div>
                    })}
                </dl>
            </Card>

            <Card title="Technical Details">
                <details class="group">
                    <summary class="cursor-pointer text-sm text-muted-foreground">
                        "Show IDs, hashes, and internal metadata"
                    </summary>
                    <div class="mt-3 space-y-3">
                        <CopyableId id=adapter_id.clone() truncate=28 />
                        <DetailGrid class="bg-muted/20 p-3 rounded-md border text-sm text-foreground">
                            <DetailItem label="Adapter ID" value=adapter_id.to_string() mono=true is_id=true />
                            <DetailItem label="Skill Fingerprint" value=hash_b3 mono=true />
                            {repo_id.clone().map(|rid| view! {
                                <DetailItem label="Repository ID" value=rid mono=true />
                            })}
                        </DetailGrid>
                    </div>
                </details>
            </Card>

            // Update Center — stage rail + lifecycle transition controls
            <Card title="Versions">
                <PromotionStageRail current_state=lifecycle_state_for_controls.clone() />
                <AdapterLifecycleControls
                    adapter_id=adapter_id_for_lifecycle
                    adapter_name=adapter_name_for_lifecycle
                    current_state=lifecycle_state_for_controls
                    on_transition=Callback::new(move |()| {
                        if let Some(refetch) = on_refetch.as_ref() {
                            refetch.run(());
                        }
                    })
                />
            </Card>

            // Version Promotion (shown when repo_id is available)
            {repo_id_for_versions.map(|rid| view! {
                <AdapterVersionPromotionSection repo_id=rid />
            })}

            // Actions
            <Card title="Actions">
                <div class="adapter-detail-actions">
                    // Pin/Unpin button
                    {on_toggle_pin.map(|callback| {
                        let id = adapter_id_for_pin.clone();
                        let label = if is_pinned { "Unpin Skill" } else { "Pin Skill" };
                        view! {
                            <Button
                                variant=if is_pinned { ButtonVariant::Secondary } else { ButtonVariant::Outline }
                                on_click=Callback::new(move |_| callback.run(id.clone()))
                            >
                                <span class="flex items-center gap-2">
                                    <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                                        <path d="M5 5a2 2 0 012-2h6a2 2 0 012 2v2h2a1 1 0 011 1v1a1 1 0 01-1 1h-1v5a3 3 0 01-3 3H7a3 3 0 01-3-3v-5H3a1 1 0 01-1-1V8a1 1 0 011-1h2V5z"/>
                                    </svg>
                                    {label}
                                </span>
                            </Button>
                        }
                    })}

                    // Load button (shown if unloaded/cold and callback provided)
                    {on_load.and_then(|callback| {
                        can_load.then(|| {
                            let id = adapter_id_for_load.clone();
                            view! {
                                <Button
                                    variant=ButtonVariant::Primary
                                    on_click=Callback::new(move |_| callback.run(id.clone()))
                                >
                                    <span class="flex items-center gap-2">
                                        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
                                        </svg>
                                        "Load Skill"
                                    </span>
                                </Button>
                            }
                        })
                    })}

                    // Unload button (shown if hot/warm/resident and callback provided)
                    {on_unload.and_then(|callback| {
                        can_unload.then(|| {
                            let id = adapter_id_for_unload.clone();
                            view! {
                                <Button
                                    variant=ButtonVariant::Destructive
                                    on_click=Callback::new(move |_| callback.run(id.clone()))
                                >
                                    <span class="flex items-center gap-2">
                                        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/>
                                        </svg>
                                        "Unload Skill"
                                    </span>
                                </Button>
                            }
                        })
                    })}
                </div>
            </Card>
        </div>
    }
}

/// Returns a `BadgeVariant` for a trust state string.
fn trust_state_badge_variant(trust_state: &str) -> BadgeVariant {
    match trust_state {
        "allowed" => BadgeVariant::Success,
        "warn" => BadgeVariant::Warning,
        "blocked" | "blocked_regressed" => BadgeVariant::Destructive,
        _ => BadgeVariant::Secondary,
    }
}

fn describe_adapter_compatibility(framework: Option<&str>, languages: &[String]) -> String {
    let language_summary = match languages {
        [] => "general workflows".to_string(),
        [one] => one.clone(),
        [first, second] => format!("{} and {}", first, second),
        [first, second, rest @ ..] => {
            format!("{}, {}, and {} more", first, second, rest.len())
        }
    };

    match framework {
        Some(framework_name) if !framework_name.trim().is_empty() => format!(
            "Designed for {} projects with {}.",
            framework_name, language_summary
        ),
        _ if !languages.is_empty() => format!("Designed for {}.", language_summary),
        _ => "Compatibility metadata is limited in the current adapter payload.".to_string(),
    }
}

fn lifecycle_readiness_text(state: LifecycleState, runtime_state: Option<&str>) -> String {
    let runtime_label = runtime_state_label(runtime_state);
    match state {
        LifecycleState::Active => format!(
            "This adapter is serving production traffic now. Runtime status: {}.",
            runtime_label
        ),
        LifecycleState::Staging => format!(
            "This adapter is reviewed and ready for promotion. Runtime status: {}.",
            runtime_label
        ),
        LifecycleState::Deprecated => format!(
            "This adapter is paused. Run checkout to reactivate when needed. Runtime status: {}.",
            runtime_label
        ),
        LifecycleState::Retired => format!(
            "This adapter is retired and should stay out of production. Runtime status: {}.",
            runtime_label
        ),
        LifecycleState::Draft | LifecycleState::Training => format!(
            "This adapter is still preparing for review. Runtime status: {}.",
            runtime_label
        ),
        _ => format!("Runtime status: {}.", runtime_label),
    }
}

fn lifecycle_stage_label(state: LifecycleState) -> &'static str {
    match state {
        LifecycleState::Draft => "Draft",
        LifecycleState::Staging => "Reviewed",
        LifecycleState::Active => "Production",
        LifecycleState::Deprecated => "Paused",
        LifecycleState::Retired => "Retired",
        _ => "Unknown",
    }
}

fn runtime_state_label(runtime_state: Option<&str>) -> &'static str {
    match runtime_state {
        Some("hot") => "Ready",
        Some("warm") => "Warming",
        Some("cold") => "Standby",
        Some("resident") => "Pinned in Memory",
        Some("unloaded") => "Not Loaded",
        _ => "Unknown",
    }
}

fn release_state_badge_variant(release_state: &str) -> BadgeVariant {
    match release_state {
        "promoted" => BadgeVariant::Success,
        "draft" => BadgeVariant::Secondary,
        "candidate" => BadgeVariant::Warning,
        "deprecated" => BadgeVariant::Default,
        _ => BadgeVariant::Default,
    }
}

fn release_state_label(release_state: &str) -> &'static str {
    match release_state {
        "draft" => "Draft",
        "candidate" => "Reviewed",
        "promoted" => "Production",
        "deprecated" => "Archived",
        _ => "Version",
    }
}

fn timeline_event_label(event_type: &str) -> String {
    if let Some(state) = event_type.strip_prefix("state_change:") {
        return format!("State change to {}", humanize(state));
    }
    humanize(event_type)
}

fn training_feed_target(repo_id: &str, return_to: &str, context: Option<(&str, &str)>) -> String {
    match context {
        Some((branch, source_version_id))
            if !branch.trim().is_empty() && !source_version_id.trim().is_empty() =>
        {
            format!(
                "/training?open_wizard=1&repo_id={}&branch={}&source_version_id={}&return_to={}",
                encode(repo_id),
                encode(branch),
                encode(source_version_id),
                encode(return_to)
            )
        }
        _ => format!(
            "/training?open_wizard=1&repo_id={}&return_to={}",
            encode(repo_id),
            encode(return_to)
        ),
    }
}

/// Adapter version promotion section.
///
/// Fetches versions for a repository and allows promoting, checking out,
/// and inspecting trust state and dataset lineage per version.
#[component]
fn AdapterVersionPromotionSection(
    /// Repository ID to fetch versions for
    #[prop(into)]
    repo_id: String,
) -> impl IntoView {
    const ARIA_RUN_PROMOTE_VERSION: &str = "Run Promote for this version";
    const ARIA_RUN_CHECKOUT_VERSION: &str = "Run Checkout for this version";
    const ARIA_FEED_DATASET_VERSION: &str = "Feed Dataset from this version context";

    let client = use_api_client();
    let notifications = use_notifications();
    let navigate = use_navigate();
    let location = use_location();

    // Versions list signal, loaded on mount
    let versions = RwSignal::new(Vec::<crate::api::types::AdapterVersionSummary>::new());
    let versions_loading = RwSignal::new(true);
    let versions_error = RwSignal::new(None::<String>);
    let timeline = RwSignal::new(Vec::<crate::api::types::TimelineEvent>::new());
    let timeline_loading = RwSignal::new(true);
    let timeline_error = RwSignal::new(None::<String>);

    // Promotion dialog state
    let show_promote_dialog = RwSignal::new(false);
    let promote_target = RwSignal::new(None::<(String, String, String)>); // (version_id, repo_id, version_label)
    let promote_loading = RwSignal::new(false);

    // Checkout dialog state
    let show_checkout_dialog = RwSignal::new(false);
    // (version_id, repo_id, branch, version_label)
    let checkout_target = RwSignal::new(None::<(String, String, String, String)>);
    let checkout_loading = RwSignal::new(false);

    // Selector-based version resolution state
    let version_selector = RwSignal::new(String::new());
    let resolve_loading = RwSignal::new(false);
    let resolve_error = RwSignal::new(None::<String>);
    let resolved_version_id = RwSignal::new(None::<String>);

    // Dataset lineage expand state (tracked by version id)
    let expanded_lineage = RwSignal::new(None::<String>);

    // Feed new training data directly from version controls.
    // Optional context carries (branch, source_version_id) for branch-aware evolution.
    let start_dataset_feed = {
        let navigate = navigate.clone();
        let pathname = location.pathname;
        let repo_id = repo_id.clone();
        Callback::new(move |_| {
            let return_to = pathname
                .try_get_untracked()
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| "/adapters".to_string());
            let current_versions = versions.try_get_untracked().unwrap_or_default();
            let selected_context =
                resolved_version_id
                    .try_get_untracked()
                    .flatten()
                    .and_then(|selected_id| {
                        current_versions
                            .iter()
                            .find(|v| v.id == selected_id)
                            .map(|v| (v.branch.clone(), v.id.clone()))
                    });
            let promoted_context = current_versions
                .iter()
                .find(|v| v.release_state == "promoted")
                .map(|v| (v.branch.clone(), v.id.clone()));
            let context = selected_context.or(promoted_context);
            let target = if let Some((branch, source_version_id)) = context {
                training_feed_target(&repo_id, &return_to, Some((&branch, &source_version_id)))
            } else {
                training_feed_target(&repo_id, &return_to, None)
            };
            navigate(&target, Default::default());
        })
    };

    let start_dataset_feed_for_version = {
        let navigate = navigate.clone();
        let pathname = location.pathname;
        let repo_id = repo_id.clone();
        Callback::new(move |(branch, source_version_id): (String, String)| {
            let return_to = pathname
                .try_get_untracked()
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| "/adapters".to_string());
            let target =
                training_feed_target(&repo_id, &return_to, Some((&branch, &source_version_id)));
            navigate(&target, Default::default());
        })
    };

    // Fetch versions on mount
    {
        let client = Arc::clone(&client);
        let repo_id = repo_id.clone();
        spawn_local(async move {
            match client.list_adapter_versions(&repo_id).await {
                Ok(v) => {
                    versions.set(v);
                    versions_loading.set(false);
                }
                Err(e) => {
                    versions_error.set(Some(e.to_string()));
                    versions_loading.set(false);
                }
            }
        });
    }
    {
        let client = Arc::clone(&client);
        let repo_id = repo_id.clone();
        spawn_local(async move {
            match client.get_repo_timeline(&repo_id).await {
                Ok(events) => {
                    timeline.set(events);
                    timeline_error.set(None);
                    timeline_loading.set(false);
                }
                Err(e) => {
                    timeline_error.set(Some(e.user_message()));
                    timeline_loading.set(false);
                }
            }
        });
    }

    // Handle selector resolve action
    let handle_resolve = {
        let client = Arc::clone(&client);
        let repo_id = repo_id.clone();
        Callback::new(move |()| {
            let selector = version_selector
                .try_get_untracked()
                .unwrap_or_default()
                .trim()
                .to_string();
            if selector.is_empty() {
                resolved_version_id.set(None);
                resolve_error.set(Some(
                    "Enter a version reference. Examples: tag:latest-stable, main@v3, main"
                        .to_string(),
                ));
                return;
            }

            resolve_loading.set(true);
            resolve_error.set(None);
            let client = Arc::clone(&client);
            let repo_id = repo_id.clone();
            spawn_local(async move {
                match client.resolve_adapter_version(&repo_id, &selector).await {
                    Ok(Some(version_id)) => {
                        resolved_version_id.set(Some(version_id));
                        resolve_loading.set(false);
                    }
                    Ok(None) => {
                        resolved_version_id.set(None);
                        resolve_error.set(Some(format!("No version matched '{}'.", selector)));
                        resolve_loading.set(false);
                    }
                    Err(err) => {
                        resolved_version_id.set(None);
                        resolve_error.set(Some(err.user_message()));
                        resolve_loading.set(false);
                    }
                }
            });
        })
    };

    // Handle promote confirmation
    let handle_promote = {
        let client = Arc::clone(&client);
        let notifications = notifications.clone();
        Callback::new(move |()| {
            let Some((version_id, rid, label)) = promote_target.try_get_untracked().flatten()
            else {
                return;
            };
            let client = Arc::clone(&client);
            let notifications = notifications.clone();
            promote_loading.set(true);
            spawn_local(async move {
                match client.promote_adapter_version(&version_id, &rid).await {
                    Ok(()) => {
                        notifications.success(
                            "Promote Completed",
                            &format!("{} is now live in Production", label),
                        );
                        show_promote_dialog.set(false);
                        promote_loading.set(false);
                        promote_target.set(None);

                        // Refresh the version list
                        if let Ok(v) = client.list_adapter_versions(&rid).await {
                            versions.set(v);
                        }
                        if let Ok(events) = client.get_repo_timeline(&rid).await {
                            timeline.set(events);
                            timeline_error.set(None);
                        }
                    }
                    Err(err) => {
                        notifications.error("Promote Failed", &err.to_string());
                        promote_loading.set(false);
                    }
                }
            });
        })
    };

    // Handle checkout confirmation
    let handle_checkout = {
        let client = Arc::clone(&client);
        let notifications = notifications.clone();
        Callback::new(move |()| {
            let Some((version_id, rid, branch, label)) =
                checkout_target.try_get_untracked().flatten()
            else {
                return;
            };
            let client = Arc::clone(&client);
            let notifications = notifications.clone();
            checkout_loading.set(true);
            spawn_local(async move {
                match client
                    .checkout_adapter_version(&rid, &branch, &version_id)
                    .await
                {
                    Ok(()) => {
                        notifications.success(
                            "Version Checked Out",
                            &format!(
                                "{} is now checked out as the active production version",
                                label
                            ),
                        );
                        show_checkout_dialog.set(false);
                        checkout_loading.set(false);
                        checkout_target.set(None);

                        // Refresh the version list
                        if let Ok(v) = client.list_adapter_versions(&rid).await {
                            versions.set(v);
                        }
                        if let Ok(events) = client.get_repo_timeline(&rid).await {
                            timeline.set(events);
                            timeline_error.set(None);
                        }
                    }
                    Err(err) => {
                        notifications.error("Checkout Failed", &err.to_string());
                        checkout_loading.set(false);
                    }
                }
            });
        })
    };

    view! {
        <Card title="Versions">
            {move || {
                if versions_loading.try_get().unwrap_or(true) {
                    return view! {
                        <div class="flex items-center gap-2 py-2">
                            <Spinner />
                            <span class="text-sm text-muted-foreground">"Loading versions..."</span>
                        </div>
                    }.into_any();
                }

                if let Some(err) = versions_error.try_get().flatten() {
                    return view! {
                        <p class="text-sm text-muted-foreground">{format!("Could not load update history: {}", err)}</p>
                    }.into_any();
                }

                let vers = versions.try_get().unwrap_or_default();
                if vers.is_empty() {
                    return view! {
                        <p class="text-sm text-muted-foreground">
                            "No saved versions for this skill yet. Feed a dataset to create the first version."
                        </p>
                    }.into_any();
                }

                let resolve_error_msg = resolve_error.try_get().flatten();
                let resolved_id = resolved_version_id.try_get().flatten();
                let resolved_version = resolved_id
                    .as_ref()
                    .and_then(|id| vers.iter().find(|v| v.id == *id).cloned());
                let timeline_loading_now = timeline_loading.try_get().unwrap_or(true);
                let timeline_error_msg = timeline_error.try_get().flatten();
                let timeline_events = timeline.try_get().unwrap_or_default();
                let recommended_next = if let Some(resolved) = resolved_version.as_ref() {
                    if resolved.release_state == "deprecated" {
                        "This version is paused. Run checkout to reactivate it in Production."
                            .to_string()
                    } else if resolved.release_state == "promoted" {
                        "This version is live. Feed a new dataset to create the next revision."
                            .to_string()
                    } else if resolved.serveable {
                        "This version is reviewed and ready. Run promote when you are ready for production."
                            .to_string()
                    } else {
                        "This version is not serveable yet. Keep it in review before promoting."
                            .to_string()
                    }
                } else if vers
                    .iter()
                    .any(|v| v.release_state != "promoted" && v.serveable)
                {
                    "You have reviewed versions ready for promotion. Find one and run promote."
                        .to_string()
                } else if vers.iter().any(|v| v.release_state == "deprecated") {
                    "Need to recover fast? Find a paused version and run checkout from history."
                        .to_string()
                } else {
                    "Start by finding a version by tag or branch, then run checkout or promote, then feed-dataset."
                        .to_string()
                };

                view! {
                    <div class="space-y-3">
                        <div class="rounded-md border border-border/50 bg-muted/20 p-3 space-y-2">
                            <p class="text-sm font-medium">"Git-Style Repository Workflow"</p>
                            <p class="text-xs text-muted-foreground">
                                "Treat each adapter version like branch history: resolve selectors, promote reviewed versions, checkout prior versions, and keep dataset lineage auditable."
                            </p>
                            <div class="space-y-1">
                                <p class="text-xs font-medium text-muted-foreground">"Command map"</p>
                                <div class="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                                    <span class="rounded border border-border/60 bg-background/70 px-2 py-0.5 font-mono">"checkout <branch>@<version>"</span>
                                    <span class="rounded border border-border/60 bg-background/70 px-2 py-0.5 font-mono">"promote <version> --to production"</span>
                                    <span class="rounded border border-border/60 bg-background/70 px-2 py-0.5 font-mono">"feed-dataset --branch <branch> --from <version>"</span>
                                </div>
                                <p class="text-xs text-muted-foreground">
                                    "Use these commands as a mental model; the buttons below execute the same workflow safely."
                                </p>
                                <p class="text-xs text-muted-foreground">
                                    "Default path: resolve a version, run checkout or promote, then feed-dataset."
                                </p>
                            </div>
                            <Button
                                variant=ButtonVariant::Outline
                                size=ButtonSize::Sm
                                on_click=start_dataset_feed
                                aria_label="Feed a new dataset using selected or production version context"
                            >
                                "Feed New Dataset"
                            </Button>
                            <p class="text-xs text-muted-foreground">
                                "Feed New Dataset uses resolved version context when selected; otherwise it falls back to Production context."
                            </p>
                            <p class="text-xs font-medium text-muted-foreground">"Quick operator guide"</p>
                            <ol
                                class="text-xs text-muted-foreground space-y-1"
                                style="list-style: decimal; padding-left: 1.1rem;"
                            >
                                <li>"Find a version by tag or branch, then confirm trust and lineage."</li>
                                <li>"Run checkout for fast recovery, or run promote for reviewed versions."</li>
                                <li>"Feed a new dataset to continue the same branch/version history."</li>
                            </ol>
                        </div>

                        <div class="rounded-md border border-border/50 bg-muted/15 p-3" role="status" aria-live="polite">
                            <p class="text-sm font-medium">"Recommended Next Action"</p>
                            <p class="text-xs text-muted-foreground">{recommended_next}</p>
                        </div>

                        <div class="rounded-md border border-border/50 bg-muted/20 p-3 space-y-2">
                            <p class="text-sm font-medium">"Promotion Path"</p>
                            <div class="flex flex-wrap items-center gap-2 text-xs">
                                <Badge variant=BadgeVariant::Secondary>"Draft"</Badge>
                                <span class="text-muted-foreground">"→"</span>
                                <Badge variant=BadgeVariant::Warning>"Reviewed"</Badge>
                                <span class="text-muted-foreground">"→"</span>
                                <Badge variant=BadgeVariant::Success>"Production"</Badge>
                            </div>
                            <p class="text-xs text-muted-foreground">
                                "Only reviewed, serveable versions can run promote to production. Checkout keeps commit-like history reversible without losing lineage."
                            </p>
                            <p class="text-xs text-muted-foreground">
                                "Promotion state is mutable. Each version ID and its lineage are immutable artifacts once written."
                            </p>
                        </div>

                        <div class="rounded-md border border-border/50 bg-muted/20 p-3 space-y-2">
                            <div class="flex items-center justify-between gap-2">
                                <p class="text-sm font-medium">"Repository Command Timeline"</p>
                                <span class="text-[11px] text-muted-foreground">"Latest first"</span>
                            </div>
                            <p class="text-xs text-muted-foreground">
                                "Every promote and checkout action is recorded here so operators can verify history before feeding the next dataset."
                            </p>
                            {if timeline_loading_now {
                                view! {
                                    <div class="flex items-center gap-2 py-1">
                                        <Spinner />
                                        <span class="text-xs text-muted-foreground">"Loading command timeline..."</span>
                                    </div>
                                }.into_any()
                            } else if let Some(err) = timeline_error_msg {
                                view! {
                                    <p class="text-xs text-destructive">{format!("Timeline unavailable: {}", err)}</p>
                                }.into_any()
                            } else if timeline_events.is_empty() {
                                view! {
                                    <p class="text-xs text-muted-foreground">
                                        "No command events yet. Run promote or checkout to start history."
                                    </p>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="space-y-2">
                                        {timeline_events.into_iter().take(8).map(|event| {
                                            let label = timeline_event_label(&event.event_type);
                                            view! {
                                                <div class="rounded border border-border/50 bg-background/70 p-2 space-y-1">
                                                    <div class="flex flex-wrap items-center justify-between gap-2">
                                                        <span class="text-xs font-medium">{label}</span>
                                                        <span class="text-[11px] text-muted-foreground">{event.timestamp.clone()}</span>
                                                    </div>
                                                    <p class="text-xs text-muted-foreground">{event.description.clone()}</p>
                                                </div>
                                            }
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }}
                        </div>

                        <div class="rounded-md border border-border/40 p-3">
                            <div class="flex flex-col gap-2 sm:flex-row sm:items-center">
                                <input
                                    type="text"
                                    class="input flex-1"
                                    placeholder="Find a version (tag or branch)"
                                    aria_label="Search versions"
                                    prop:value=move || version_selector.try_get().unwrap_or_default()
                                    on:input=move |ev| {
                                        version_selector.set(event_target_value(&ev));
                                        resolve_error.set(None);
                                    }
                                />
                                <Button
                                    variant=ButtonVariant::Outline
                                    size=ButtonSize::Sm
                                    on_click=handle_resolve
                                    aria_label="Find adapter version by selector"
                                    disabled=Signal::derive(move || {
                                        version_selector
                                            .try_get()
                                            .map(|selector| selector.trim().is_empty())
                                            .unwrap_or(true)
                                    })
                                    loading=Signal::derive(move || resolve_loading.try_get().unwrap_or(false))
                                >
                                    "Find"
                                </Button>
                            </div>
                            <p class="mt-2 text-xs text-muted-foreground">
                                "Examples: tag:latest-stable, main@v3, main"
                            </p>
                            {resolve_error_msg.map(|err| view! {
                                <p class="mt-2 text-xs text-destructive">{err}</p>
                            })}
                            {resolved_version.map(|resolved| {
                                let resolved_label = resolved
                                    .display_name
                                    .clone()
                                    .unwrap_or_else(|| resolved.version.clone());
                                let resolved_is_promoted = resolved.release_state == "promoted";
                                let resolved_is_deprecated = resolved.release_state == "deprecated";
                                let resolved_is_serveable = resolved.serveable;
                                let resolved_repo_display = resolved.repo_id.clone();
                                let resolved_branch_display = resolved.branch.clone();
                                let resolved_branch_for_feed_msg = resolved.branch.clone();
                                let feed_branch = resolved.branch.clone();
                                let feed_vid = resolved.id.clone();

                                let promote_vid = resolved.id.clone();
                                let promote_rid = resolved.repo_id.clone();
                                let promote_label = resolved_label.clone();

                                let checkout_vid = resolved.id.clone();
                                let checkout_rid = resolved.repo_id.clone();
                                let checkout_branch = resolved.branch.clone();
                                let checkout_label = resolved_label.clone();

                                view! {
                                    <div class="mt-3 flex flex-wrap items-center gap-2">
                                        <span class="text-xs text-muted-foreground">"Selected version:"</span>
                                        <span class="text-sm font-medium">{resolved_label.clone()}</span>
                                        <Badge variant=BadgeVariant::Secondary>{resolved_branch_display.clone()}</Badge>
                                        <Button
                                            variant=ButtonVariant::Secondary
                                            size=ButtonSize::Sm
                                            aria_label=ARIA_FEED_DATASET_VERSION
                                            on_click=Callback::new(move |_| {
                                                start_dataset_feed_for_version.run((feed_branch.clone(), feed_vid.clone()));
                                            })
                                        >
                                            "Feed Dataset from This Version"
                                        </Button>
                                        <span class="text-xs text-muted-foreground">
                                            {format!(
                                                "Training opens with repo '{}', branch '{}', and source version context prefilled.",
                                                resolved_repo_display,
                                                resolved_branch_for_feed_msg
                                            )}
                                        </span>
                                        {(!resolved_is_promoted && resolved_is_serveable).then(|| {
                                            let vid = promote_vid.clone();
                                            let rid = promote_rid.clone();
                                            let label = promote_label.clone();
                                            view! {
                                                <Button
                                                    variant=ButtonVariant::Outline
                                                    size=ButtonSize::Sm
                                                    aria_label=ARIA_RUN_PROMOTE_VERSION
                                                    on_click=Callback::new(move |_| {
                                                        promote_target.set(Some((vid.clone(), rid.clone(), label.clone())));
                                                        show_promote_dialog.set(true);
                                                    })
                                                >
                                                    "Run Promote"
                                                </Button>
                                            }
                                        })}
                                        {resolved_is_deprecated.then(|| {
                                            let vid = checkout_vid.clone();
                                            let rid = checkout_rid.clone();
                                            let branch = checkout_branch.clone();
                                            let label = checkout_label.clone();
                                            view! {
                                                <Button
                                                    variant=ButtonVariant::Destructive
                                                    size=ButtonSize::Sm
                                                    aria_label=ARIA_RUN_CHECKOUT_VERSION
                                                    on_click=Callback::new(move |_| {
                                                        checkout_target.set(Some((vid.clone(), rid.clone(), branch.clone(), label.clone())));
                                                        show_checkout_dialog.set(true);
                                                    })
                                                >
                                                    "Run Checkout"
                                                </Button>
                                            }
                                        })}
                                        {resolved_is_promoted.then(|| view! {
                                            <span class="version-active-label">"Live in Production"</span>
                                        })}
                                        {(!resolved_is_promoted && !resolved_is_serveable && !resolved_is_deprecated).then(|| view! {
                                            <span class="version-not-serveable">"Needs review before production"</span>
                                        })}
                                    </div>
                                }
                            })}
                        </div>

                        <div class="version-list">
                            {vers.into_iter().map(|v| {
                                let version_label = v.display_name.clone()
                                    .unwrap_or_else(|| v.version.clone());
                                let is_promoted = v.release_state == "promoted";
                                let is_deprecated = v.release_state == "deprecated";
                                let is_serveable = v.serveable;
                                let is_resolved = resolved_id.as_deref() == Some(v.id.as_str());
                                let trust_state = v.adapter_trust_state.clone();
                                let trust_state_label = status_display_label(&trust_state);
                                let state_variant = release_state_badge_variant(v.release_state.as_str());
                                let release_state_text = release_state_label(v.release_state.as_str());

                                let vid = v.id.clone();
                                let rid = v.repo_id.clone();
                                let branch = v.branch.clone();
                                let label_for_dialog = version_label.clone();
                                let branch_display = v.branch.clone();

                                // Dataset lineage data
                                let dataset_ids = v.dataset_version_ids.clone().unwrap_or_default();
                                let dataset_trust = v.dataset_version_trust.clone().unwrap_or_default();
                                let has_dataset_lineage = !dataset_ids.is_empty();
                                let dataset_count = dataset_ids.len();
                                let vid_for_lineage = v.id.clone();

                                // Serveable indicator
                                let serveable_reason = v.serveable_reason.clone()
                                    .unwrap_or_else(|| {
                                        if is_serveable {
                                            "Ready for production".to_string()
                                        } else {
                                            "Not ready for production".to_string()
                                        }
                                    });

                                view! {
                                    <div
                                        class="version-item"
                                        style=if is_resolved {
                                            "box-shadow: inset 0 0 0 1px rgba(14, 165, 233, 0.6); background-color: rgba(14, 165, 233, 0.08);"
                                        } else {
                                            ""
                                        }
                                    >
                                        // Main row: label, badges, actions
                                        <div class="version-item-row">
                                            <div class="version-item-info">
                                                <span class="version-item-label">{version_label.clone()}</span>
                                                <Badge variant=state_variant>{release_state_text}</Badge>
                                                <Badge variant=BadgeVariant::Secondary>"Immutable Artifact"</Badge>
                                                {is_resolved.then(|| view! {
                                                    <Badge variant=BadgeVariant::Secondary>"Resolved"</Badge>
                                                })}
                                                // Trust state badge
                                                {(!trust_state.is_empty()).then(|| {
                                                    let variant = trust_state_badge_variant(&trust_state);
                                                    view! {
                                                        <Badge variant=variant>{trust_state_label.clone()}</Badge>
                                                    }
                                                })}
                                                // Serveable indicator
                                                <span
                                                    class={if is_serveable { "version-serveable-icon version-serveable-yes" } else { "version-serveable-icon version-serveable-no" }}
                                                    title=serveable_reason
                                                >
                                                    {if is_serveable {
                                                        view! { <svg class="version-check-icon" viewBox="0 0 20 20" fill="currentColor"><path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd"/></svg> }.into_any()
                                                    } else {
                                                        view! { <svg class="version-check-icon" viewBox="0 0 20 20" fill="currentColor"><path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd"/></svg> }.into_any()
                                                    }}
                                                </span>
                                                <span class="version-item-branch">{branch_display}</span>
                                            </div>
                                            <div class="version-item-actions">
                                                // Promote button (non-promoted, serveable versions)
                                                {(!is_promoted && is_serveable).then(|| {
                                                    let vid = vid.clone();
                                                    let rid = rid.clone();
                                                    let label = label_for_dialog.clone();
                                                    view! {
                                                        <Button
                                                            variant=ButtonVariant::Outline
                                                            size=ButtonSize::Sm
                                                            aria_label=ARIA_RUN_PROMOTE_VERSION
                                                            on_click=Callback::new(move |_| {
                                                                promote_target.set(Some((vid.clone(), rid.clone(), label.clone())));
                                                                show_promote_dialog.set(true);
                                                            })
                                                        >
                                                            "Run Promote"
                                                        </Button>
                                                    }
                                                })}
                                                // Checkout button (shown on deprecated versions)
                                                {is_deprecated.then(|| {
                                                    let vid = vid.clone();
                                                    let rid = rid.clone();
                                                    let branch = branch.clone();
                                                    let label = label_for_dialog.clone();
                                                    view! {
                                                        <Button
                                                            variant=ButtonVariant::Destructive
                                                            size=ButtonSize::Sm
                                                            aria_label=ARIA_RUN_CHECKOUT_VERSION
                                                            on_click=Callback::new(move |_| {
                                                                checkout_target.set(Some((vid.clone(), rid.clone(), branch.clone(), label.clone())));
                                                                show_checkout_dialog.set(true);
                                                            })
                                                        >
                                                            "Run Checkout"
                                                        </Button>
                                                    }
                                                })}
                                                // Not serveable label (non-promoted, non-deprecated, not serveable)
                                                {(!is_promoted && !is_serveable && !is_deprecated).then(|| {
                                                    let reason = v.serveable_reason.clone()
                                                        .unwrap_or_else(|| "Not ready for production".to_string());
                                                    view! {
                                                        <span class="version-not-serveable" title=reason>
                                                            "Needs review"
                                                        </span>
                                                    }
                                                })}
                                                // Active label for promoted
                                                {is_promoted.then(|| view! {
                                                    <span class="version-active-label">"In Production"</span>
                                                })}
                                                // Dataset lineage toggle
                                                {has_dataset_lineage.then(|| {
                                                    let vid_toggle = vid_for_lineage.clone();
                                                    let lineage_button_label = if dataset_count == 1 {
                                                        "Show upstream dataset (1)".to_string()
                                                    } else {
                                                        format!("Show upstream datasets ({})", dataset_count)
                                                    };
                                                    view! {
                                                        <button
                                                            type="button"
                                                            class="version-lineage-toggle"
                                                            aria-label="Toggle dataset lineage evidence for this version"
                                                            on:click=move |_| {
                                                                let current = expanded_lineage.try_get().flatten();
                                                                if current.as_deref() == Some(&vid_toggle) {
                                                                    expanded_lineage.set(None);
                                                                } else {
                                                                    expanded_lineage.set(Some(vid_toggle.clone()));
                                                                }
                                                            }
                                                            title="Evidence lineage"
                                                        >
                                                            <svg class="version-lineage-icon" viewBox="0 0 20 20" fill="currentColor">
                                                                <path d="M3 12v3c0 1.657 3.134 3 7 3s7-1.343 7-3v-3c0 1.657-3.134 3-7 3s-7-1.343-7-3z"/>
                                                                <path d="M3 7v3c0 1.657 3.134 3 7 3s7-1.343 7-3V7c0 1.657-3.134 3-7 3S3 8.657 3 7z"/>
                                                                <path d="M17 5c0 1.657-3.134 3-7 3S3 6.657 3 5s3.134-3 7-3 7 1.343 7 3z"/>
                                                            </svg>
                                                            <span class="text-xs">{lineage_button_label}</span>
                                                        </button>
                                                    }
                                                })}
                                            </div>
                                        </div>
                                        // Collapsible dataset lineage section
                                        {has_dataset_lineage.then(|| {
                                            let vid_check = vid_for_lineage.clone();
                                            let ds_ids = dataset_ids.clone();
                                            let ds_trust = dataset_trust.clone();
                                            view! {
                                                <div
                                                    class="version-lineage-section"
                                                    style:display=move || {
                                                        if expanded_lineage.try_get().flatten().as_deref() == Some(&vid_check) {
                                                            "block"
                                                        } else {
                                                            "none"
                                                        }
                                                    }
                                                >
                                                    <span class="version-lineage-title">"Upstream Dataset Versions"</span>
                                                    <div class="version-lineage-list">
                                                        {ds_ids.into_iter().map(|ds_id| {
                                                            let trust_entry = ds_trust.iter()
                                                                .find(|t| t.dataset_version_id == ds_id);
                                                            let trust_label = trust_entry
                                                                .and_then(|t| t.trust_at_training_time.clone())
                                                                .unwrap_or_else(|| "unknown".to_string());
                                                            let trust_label_display = status_display_label(&trust_label);
                                                            let trust_variant = trust_state_badge_variant(&trust_label);
                                                            let dataset_id = trust_entry.and_then(|t| t.dataset_id.clone());
                                                            let dataset_name = trust_entry.and_then(|t| t.dataset_name.clone());
                                                            view! {
                                                                <div class="version-lineage-item">
                                                                    {if let (Some(did), Some(name)) = (dataset_id, dataset_name) {
                                                                        let href = format!("/datasets/{}", did);
                                                                        view! {
                                                                            <a href=href class="text-primary hover:underline font-medium">{name}</a>
                                                                            <span class="text-xs text-muted-foreground font-mono ml-1">{ds_id}</span>
                                                                        }.into_any()
                                                                    } else {
                                                                        view! {
                                                                            <CopyableId id=ds_id.clone() truncate=20 />
                                                                        }.into_any()
                                                                    }}
                                                                    <Badge variant=trust_variant>{trust_label_display}</Badge>
                                                                </div>
                                                            }
                                                        }).collect_view()}
                                                    </div>
                                                </div>
                                            }
                                        })}
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    </div>
                }.into_any()
            }}

            // Promotion confirmation dialog
            <ConfirmationDialog
                open=show_promote_dialog
                title="Run Promote to Production"
                description="This runs promote on the selected reviewed version for Production. The prior production version stays in history and can be checked out.".to_string()
                severity=ConfirmationSeverity::Normal
                confirm_text="Run Promote"
                on_confirm=handle_promote
                loading=Signal::derive(move || promote_loading.try_get().unwrap_or(false))
            />

            // Checkout confirmation dialog
            <ConfirmationDialog
                open=show_checkout_dialog
                title="Run Checkout Previous Version"
                description="This checks out the selected version as the active Production version and preserves a full audit trail.".to_string()
                severity=ConfirmationSeverity::Destructive
                confirm_text="Run Checkout"
                on_confirm=handle_checkout
                loading=Signal::derive(move || checkout_loading.try_get().unwrap_or(false))
            />
        </Card>
    }
}

/// Promotion stage rail: Draft → Reviewed → Production
///
/// Shows the three-stage promotion pipeline with the current stage highlighted.
/// Displayed inside the Update Center card in the adapter detail panel.
#[component]
fn PromotionStageRail(
    /// Current lifecycle state as a string (e.g. "draft", "staging", "active").
    #[prop(into)]
    current_state: String,
) -> impl IntoView {
    #[derive(Clone, PartialEq)]
    enum StageStatus {
        Done,
        Current,
        Upcoming,
    }

    let stages: &[(&str, &str, &str)] = &[
        ("draft", "Draft", "skill created"),
        ("staging", "Reviewed", "approved by team"),
        ("active", "Production", "live and serving"),
    ];

    let current = current_state.to_lowercase();
    // Map state to stage index
    let current_idx: usize = match current.as_str() {
        "draft" => 0,
        "staging" => 1,
        "active" => 2,
        _ => 0,
    };

    view! {
        <div class="promotion-stage-rail promotion-rail">
            {stages.iter().enumerate().map(|(idx, (_, label, hint))| {
                let status = if idx < current_idx {
                    StageStatus::Done
                } else if idx == current_idx {
                    StageStatus::Current
                } else {
                    StageStatus::Upcoming
                };

                let circle_class = match &status {
                    StageStatus::Done    => "stage-dot stage-dot--done",
                    StageStatus::Current => "stage-dot stage-dot--current",
                    StageStatus::Upcoming => "stage-dot stage-dot--upcoming",
                };

                let label_class = match &status {
                    StageStatus::Done    => "stage-label stage-label--done",
                    StageStatus::Current => "stage-label stage-label--current",
                    StageStatus::Upcoming => "stage-label stage-label--upcoming",
                };

                let show_connector = idx + 1 < stages.len();

                view! {
                    <div class="promotion-stage">
                        <div class="promotion-stage-node">
                            <div class=format!("{} promotion-stage-dot", circle_class)>
                                {if status == StageStatus::Done {
                                    view! {
                                        <svg class="promotion-stage-check" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                                            <path d="M5 13l4 4L19 7"/>
                                        </svg>
                                    }.into_any()
                                } else {
                                    view! { <span>{idx + 1}</span> }.into_any()
                                }}
                            </div>
                            <div class=format!("{} promotion-stage-label-wrap", label_class)>
                                <p class="promotion-stage-label-text">{*label}</p>
                                <p class="promotion-stage-hint-text">{*hint}</p>
                            </div>
                        </div>
                        {show_connector.then(|| view! {
                            <div class="stage-connector promotion-stage-connector"></div>
                        })}
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
