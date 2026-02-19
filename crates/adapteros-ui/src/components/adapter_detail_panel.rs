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

use adapteros_api_types::{AdapterResponse, LifecycleState};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::sync::Arc;

use crate::api::use_api_client;
use crate::components::{
    AdapterLifecycleControls, Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card,
    ConfirmationDialog, ConfirmationSeverity, CopyableId, HashDisplay, ProvenanceBadge, Spinner,
    VersionTimeline,
};
use crate::contexts::use_in_flight;
use crate::signals::notifications::use_notifications;
use crate::utils::format_bytes;

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
        <div class="adapter-detail-empty">
            <div class="adapter-detail-empty-icon">
                <svg
                    class="w-12 h-12 text-muted-foreground/50"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                    xmlns="http://www.w3.org/2000/svg"
                >
                    <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="1.5"
                        d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"
                    />
                </svg>
            </div>
            <p class="adapter-detail-empty-hint">
                "Select a skill to view details, trust context, and update controls."
            </p>
        </div>
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
    let repo_id_for_timeline = repo_id.clone();

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

    view! {
        <div class="adapter-detail-content">
            // Header with close button
            <div class="adapter-detail-header">
                <div class="adapter-detail-header-info">
                    <h2 class="adapter-detail-title">{name}</h2>
                    <CopyableId id=adapter_id.clone() truncate=28 />
                </div>
                <button
                    type="button"
                    class="adapter-detail-close"
                    on:click=move |_| on_close.run(())
                    aria-label="Close detail panel"
                >
                    <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                    </svg>
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

            // Core Metadata
            <Card title="Trust Details">
                <dl class="adapter-detail-metadata">
                    <div class="adapter-detail-metadata-item">
                        <dt>"Skill Fingerprint"</dt>
                        <dd><HashDisplay hash=hash_b3 /></dd>
                    </div>
                    <div class="adapter-detail-metadata-item">
                        <dt>"Version"</dt>
                        <dd>{version}</dd>
                    </div>
                    <div class="adapter-detail-metadata-item">
                        <dt>"Tier"</dt>
                        <dd>{tier}</dd>
                    </div>
                    <div class="adapter-detail-metadata-item">
                        <dt>"Category"</dt>
                        <dd>{category}</dd>
                    </div>
                    <div class="adapter-detail-metadata-item">
                        <dt>"Coverage"</dt>
                        <dd>{scope}</dd>
                    </div>
                    <div class="adapter-detail-metadata-item">
                        <dt>"Capacity"</dt>
                        <dd>{rank.to_string()}</dd>
                    </div>
                    {memory_bytes.map(|bytes| view! {
                        <div class="adapter-detail-metadata-item">
                            <dt>"Memory Usage"</dt>
                            <dd>{format_bytes(bytes)}</dd>
                        </div>
                    })}
                </dl>
            </Card>

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

            // Update Center — stage rail + lifecycle transition controls
            <Card title="Update Center">
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
            {repo_id.map(|rid| view! {
                <AdapterVersionPromotionSection repo_id=rid />
            })}

            // Version History Timeline (shown when repo_id is available)
            {repo_id_for_timeline.map(|rid| view! {
                <VersionTimeline repo_id=rid />
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

/// Adapter version promotion section.
///
/// Fetches versions for a repository and allows promoting, rolling back,
/// and inspecting trust state and dataset lineage per version.
#[component]
fn AdapterVersionPromotionSection(
    /// Repository ID to fetch versions for
    #[prop(into)]
    repo_id: String,
) -> impl IntoView {
    let client = use_api_client();
    let notifications = use_notifications();

    // Versions list signal, loaded on mount
    let versions = RwSignal::new(Vec::<crate::api::types::AdapterVersionSummary>::new());
    let versions_loading = RwSignal::new(true);
    let versions_error = RwSignal::new(None::<String>);

    // Promotion dialog state
    let show_promote_dialog = RwSignal::new(false);
    let promote_target = RwSignal::new(None::<(String, String, String)>); // (version_id, repo_id, version_label)
    let promote_loading = RwSignal::new(false);

    // Rollback dialog state
    let show_rollback_dialog = RwSignal::new(false);
    // (version_id, repo_id, branch, version_label)
    let rollback_target = RwSignal::new(None::<(String, String, String, String)>);
    let rollback_loading = RwSignal::new(false);

    // Selector-based version resolution state
    let version_selector = RwSignal::new(String::new());
    let resolve_loading = RwSignal::new(false);
    let resolve_error = RwSignal::new(None::<String>);
    let resolved_version_id = RwSignal::new(None::<String>);

    // Dataset lineage expand state (tracked by version id)
    let expanded_lineage = RwSignal::new(None::<String>);

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
                            "Moved to Production",
                            &format!("{} is now live in Production", label),
                        );
                        show_promote_dialog.set(false);
                        promote_loading.set(false);
                        promote_target.set(None);

                        // Refresh the version list
                        match client.list_adapter_versions(&rid).await {
                            Ok(v) => versions.set(v),
                            Err(_) => {} // stale list is acceptable
                        }
                    }
                    Err(err) => {
                        notifications.error("Move to Production Failed", &err.to_string());
                        promote_loading.set(false);
                    }
                }
            });
        })
    };

    // Handle rollback confirmation
    let handle_rollback = {
        let client = Arc::clone(&client);
        let notifications = notifications.clone();
        Callback::new(move |()| {
            let Some((version_id, rid, branch, label)) =
                rollback_target.try_get_untracked().flatten()
            else {
                return;
            };
            let client = Arc::clone(&client);
            let notifications = notifications.clone();
            rollback_loading.set(true);
            spawn_local(async move {
                match client
                    .rollback_adapter_version(&rid, &branch, &version_id)
                    .await
                {
                    Ok(()) => {
                        notifications.success(
                            "Version Restored",
                            &format!("{} is now restored as the active version", label),
                        );
                        show_rollback_dialog.set(false);
                        rollback_loading.set(false);
                        rollback_target.set(None);

                        // Refresh the version list
                        match client.list_adapter_versions(&rid).await {
                            Ok(v) => versions.set(v),
                            Err(_) => {} // stale list is acceptable
                        }
                    }
                    Err(err) => {
                        notifications.error("Restore Failed", &err.to_string());
                        rollback_loading.set(false);
                    }
                }
            });
        })
    };

    view! {
        <Card title="Update Center">
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
                        <p class="text-sm text-muted-foreground">"No saved versions for this skill yet."</p>
                    }.into_any();
                }

                let resolve_error_msg = resolve_error.try_get().flatten();
                let resolved_id = resolved_version_id.try_get().flatten();
                let resolved_version = resolved_id
                    .as_ref()
                    .and_then(|id| vers.iter().find(|v| v.id == *id).cloned());

                view! {
                    <div class="space-y-3">
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
                                "Only serveable reviewed versions can move to Production. Every move is auditable and reversible."
                            </p>
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
                                let resolved_branch_display = resolved.branch.clone();

                                let promote_vid = resolved.id.clone();
                                let promote_rid = resolved.repo_id.clone();
                                let promote_label = resolved_label.clone();

                                let rollback_vid = resolved.id.clone();
                                let rollback_rid = resolved.repo_id.clone();
                                let rollback_branch = resolved.branch.clone();
                                let rollback_label = resolved_label.clone();

                                view! {
                                    <div class="mt-3 flex flex-wrap items-center gap-2">
                                        <span class="text-xs text-muted-foreground">"Selected version:"</span>
                                        <span class="text-sm font-medium">{resolved_label.clone()}</span>
                                        <Badge variant=BadgeVariant::Secondary>{resolved_branch_display.clone()}</Badge>
                                        {(!resolved_is_promoted && resolved_is_serveable).then(|| {
                                            let vid = promote_vid.clone();
                                            let rid = promote_rid.clone();
                                            let label = promote_label.clone();
                                            view! {
                                                <Button
                                                    variant=ButtonVariant::Outline
                                                    size=ButtonSize::Sm
                                                    on_click=Callback::new(move |_| {
                                                        promote_target.set(Some((vid.clone(), rid.clone(), label.clone())));
                                                        show_promote_dialog.set(true);
                                                    })
                                                >
                                                    "Move to Production"
                                                </Button>
                                            }
                                        })}
                                        {resolved_is_deprecated.then(|| {
                                            let vid = rollback_vid.clone();
                                            let rid = rollback_rid.clone();
                                            let branch = rollback_branch.clone();
                                            let label = rollback_label.clone();
                                            view! {
                                                <Button
                                                    variant=ButtonVariant::Destructive
                                                    size=ButtonSize::Sm
                                                    on_click=Callback::new(move |_| {
                                                        rollback_target.set(Some((vid.clone(), rid.clone(), branch.clone(), label.clone())));
                                                        show_rollback_dialog.set(true);
                                                    })
                                                >
                                                    "Restore Version"
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
                                                {is_resolved.then(|| view! {
                                                    <Badge variant=BadgeVariant::Secondary>"Resolved"</Badge>
                                                })}
                                                // Trust state badge
                                                {(!trust_state.is_empty()).then(|| {
                                                    let variant = trust_state_badge_variant(&trust_state);
                                                    view! {
                                                        <Badge variant=variant>{trust_state.clone()}</Badge>
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
                                                            on_click=Callback::new(move |_| {
                                                                promote_target.set(Some((vid.clone(), rid.clone(), label.clone())));
                                                                show_promote_dialog.set(true);
                                                            })
                                                        >
                                                            "Promote to Production"
                                                        </Button>
                                                    }
                                                })}
                                                // Rollback button (shown on deprecated versions)
                                                {is_deprecated.then(|| {
                                                    let vid = vid.clone();
                                                    let rid = rid.clone();
                                                    let branch = branch.clone();
                                                    let label = label_for_dialog.clone();
                                                    view! {
                                                        <Button
                                                            variant=ButtonVariant::Destructive
                                                            size=ButtonSize::Sm
                                                            on_click=Callback::new(move |_| {
                                                                rollback_target.set(Some((vid.clone(), rid.clone(), branch.clone(), label.clone())));
                                                                show_rollback_dialog.set(true);
                                                            })
                                                        >
                                                            "Restore"
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
                                                    view! {
                                                        <button
                                                            type="button"
                                                            class="version-lineage-toggle"
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
                                                    <span class="version-lineage-title">"Evidence Lineage"</span>
                                                    <div class="version-lineage-list">
                                                        {ds_ids.into_iter().map(|ds_id| {
                                                            let trust_label = ds_trust.iter()
                                                                .find(|t| t.dataset_version_id == ds_id)
                                                                .and_then(|t| t.trust_at_training_time.clone())
                                                                .unwrap_or_else(|| "unknown".to_string());
                                                            let trust_variant = trust_state_badge_variant(&trust_label);
                                                            view! {
                                                                <div class="version-lineage-item">
                                                                    <CopyableId id=ds_id truncate=20 />
                                                                    <Badge variant=trust_variant>{trust_label}</Badge>
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
                title="Move Version to Production"
                description="This moves the selected reviewed version into Production. The prior production version stays in history and can be restored.".to_string()
                severity=ConfirmationSeverity::Normal
                confirm_text="Move to Production"
                on_confirm=handle_promote
                loading=Signal::derive(move || promote_loading.try_get().unwrap_or(false))
            />

            // Rollback confirmation dialog
            <ConfirmationDialog
                open=show_rollback_dialog
                title="Restore Previous Version"
                description="This restores the selected version as the active Production version and preserves a full audit trail.".to_string()
                severity=ConfirmationSeverity::Destructive
                confirm_text="Restore Version"
                on_confirm=handle_rollback
                loading=Signal::derive(move || rollback_loading.try_get().unwrap_or(false))
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
        <div class="promotion-stage-rail" style="display: flex; align-items: center; gap: 0; margin-bottom: 1rem;">
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
                    <div style="display: flex; align-items: center; flex: 1;">
                        <div style="display: flex; flex-direction: column; align-items: center; gap: 0.25rem; flex: none;">
                            <div class=circle_class style="width: 2rem; height: 2rem; border-radius: 50%; display: flex; align-items: center; justify-content: center; font-size: 0.75rem; font-weight: 600; border: 2px solid; flex-shrink: 0;">
                                {if status == StageStatus::Done {
                                    view! {
                                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" style="width: 0.875rem; height: 0.875rem;">
                                            <path d="M5 13l4 4L19 7"/>
                                        </svg>
                                    }.into_any()
                                } else {
                                    view! { <span>{idx + 1}</span> }.into_any()
                                }}
                            </div>
                            <div class=label_class style="text-align: center; min-width: 4.5rem;">
                                <p style="font-size: 0.6875rem; font-weight: 600; line-height: 1;">{*label}</p>
                                <p style="font-size: 0.625rem; margin-top: 0.125rem;">{*hint}</p>
                            </div>
                        </div>
                        {show_connector.then(|| view! {
                            <div class="stage-connector" style="height: 2px; flex: 1; margin-bottom: 1.25rem;"></div>
                        })}
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
