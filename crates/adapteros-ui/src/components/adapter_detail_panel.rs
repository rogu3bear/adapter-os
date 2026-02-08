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

use adapteros_api_types::AdapterResponse;
use leptos::prelude::*;

use crate::components::{
    AdapterLifecycleControls, Badge, BadgeVariant, Button, ButtonVariant, Card, CopyableId, Spinner,
};
use crate::contexts::use_in_flight;
use crate::utils::format_bytes;

/// Truncate a hash for display, showing first and last characters.
fn truncate_hash(hash: &str, prefix_len: usize, suffix_len: usize) -> String {
    if hash.len() <= prefix_len + suffix_len + 3 {
        hash.to_string()
    } else {
        format!(
            "{}...{}",
            &hash[..prefix_len],
            &hash[hash.len() - suffix_len..]
        )
    }
}

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
                if loading.get() {
                    return view! {
                        <div class="adapter-detail-loading">
                            <Spinner />
                        </div>
                    }.into_any();
                }

                match adapter.get() {
                    None => view! {
                        <AdapterDetailEmpty />
                    }.into_any(),
                    Some(data) => {
                        let ctx = suggestion_context
                            .map(|s| s.get())
                            .unwrap_or_default();
                        view! {
                            <AdapterDetailContent
                                adapter=data
                                suggestion_context=ctx
                                on_close=on_close
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
                "Select an adapter to view details, routing context, and lifecycle controls."
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
    let lifecycle_state_for_controls = adapter.lifecycle_state.clone();

    // Derive lifecycle badge variant
    let lifecycle_variant = match adapter.lifecycle_state.as_str() {
        "active" => BadgeVariant::Success,
        "deprecated" => BadgeVariant::Warning,
        "retired" => BadgeVariant::Destructive,
        "draft" => BadgeVariant::Secondary,
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
    let hash_display = truncate_hash(&adapter.hash_b3, 8, 8);
    let hash_full = adapter.hash_b3.clone();
    let tier = adapter.tier.clone();
    let category = adapter.category.clone().unwrap_or_else(|| "N/A".into());
    let scope = adapter.scope.clone().unwrap_or_else(|| "N/A".into());
    let lifecycle_state = adapter.lifecycle_state.clone();
    let runtime_state = adapter
        .runtime_state
        .clone()
        .unwrap_or_else(|| "unknown".into());
    let created_at = adapter.created_at.clone();
    let updated_at = adapter.updated_at.clone();
    let version = adapter.version.clone();
    let rank = adapter.rank;
    let languages = adapter.languages.clone();
    let framework = adapter.framework.clone();
    let intent = adapter.intent.clone();
    let memory_bytes = adapter.memory_bytes;
    let is_pinned = adapter.pinned.unwrap_or(false);

    // Suggestion context values
    let has_suggestion = suggestion_context.reason.is_some()
        || suggestion_context.confidence.is_some()
        || suggestion_context.gate_value.is_some();
    let suggestion_reason = suggestion_context
        .reason
        .clone()
        .unwrap_or_else(|| "Matched by router criteria".into());
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
                {move || is_in_flight.get().then(|| view! {
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
                                        <span class="adapter-detail-metric-label">"Gate Value"</span>
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
            <Card title="Metadata">
                <dl class="adapter-detail-metadata">
                    <div class="adapter-detail-metadata-item">
                        <dt>"Hash (BLAKE3)"</dt>
                        <dd class="font-mono text-sm" title=hash_full>{hash_display}</dd>
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
                        <dt>"Scope"</dt>
                        <dd>{scope}</dd>
                    </div>
                    <div class="adapter-detail-metadata-item">
                        <dt>"Rank"</dt>
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
                    <Card title="Languages & Framework">
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
                                    <span class="text-sm text-muted-foreground">"Framework: "</span>
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
            <Card title="Timeline">
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

            // Lifecycle Controls
            <Card title="Lifecycle">
                <AdapterLifecycleControls
                    adapter_id=adapter_id_for_lifecycle
                    adapter_name=adapter_name_for_lifecycle
                    current_state=lifecycle_state_for_controls
                    on_transition=Callback::new(move |()| {
                        // After a lifecycle transition, the panel data is stale.
                        // The user can close and reopen the panel to see updated state.
                        // A future enhancement could add an on_refetch prop to AdapterDetailPanel.
                    })
                />
            </Card>

            // Actions
            <Card title="Actions">
                <div class="adapter-detail-actions">
                    // Pin/Unpin button
                    {on_toggle_pin.map(|callback| {
                        let id = adapter_id_for_pin.clone();
                        let label = if is_pinned { "Unpin Adapter" } else { "Pin Adapter" };
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
                                        "Load Adapter"
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
                                        "Unload Adapter"
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
