//! Adapter visualization bar component
//!
//! Displays active adapters as "magnets" with color-coded heat levels:
//! - Hot (red): >10 uses per minute
//! - Warm (orange): 1-10 uses per minute
//! - Cold (blue): <1 use per minute
//! - Active (glowing): Currently executing inference
//!
//! ## Liquid Glass Design Compliance (PRD-UI-100)
//!
//! Follows the Liquid Glass spec from `dist/glass.css`:
//! - Tier 1 glass background for chip containers
//! - State-change-only animations (no idle animations)
//! - 200-300ms ease transitions for state changes
//! - Proper text contrast with text-shadow
//!
//! ## Deterministic Layout
//!
//! Adapter chips are sorted deterministically:
//! 1. Primary: relevance/confidence score DESC
//! 2. Secondary: stable adapter_id ASC (for tie-breaking)

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Adapter chip state for rendering
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AdapterChipState {
    /// Suggested by router, not yet selected
    #[default]
    Suggested,
    /// User has selected/activated this adapter
    Selected,
    /// User has pinned this adapter (persistent selection)
    Pinned,
    /// Adapter is disabled (incompatible, rate-limited, etc.)
    Disabled,
}

impl AdapterChipState {
    /// CSS class for the chip state
    pub fn to_css_class(&self) -> &'static str {
        match self {
            Self::Suggested => "adapter-chip-suggested",
            Self::Selected => "adapter-chip-selected",
            Self::Pinned => "adapter-chip-pinned",
            Self::Disabled => "adapter-chip-disabled",
        }
    }

    /// Human-readable label for accessibility
    pub fn to_label(&self) -> &'static str {
        match self {
            Self::Suggested => "suggested",
            Self::Selected => "selected",
            Self::Pinned => "pinned",
            Self::Disabled => "disabled",
        }
    }
}

/// Adapter state for visualization
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdapterMagnet {
    pub adapter_id: String,
    pub heat: AdapterHeat,
    pub is_active: bool,
    /// Whether this magnet is user-pinned (vs. system-reported active)
    #[serde(default)]
    pub is_pinned: bool,
}

/// Heat level classification based on usage
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterHeat {
    Hot,      // >10 uses/min
    Warm,     // 1-10 uses/min
    Cold,     // <1 use/min
    Inactive, // No recent activity
}

impl AdapterHeat {
    /// Get CSS classes for this heat level using semantic status tokens
    pub fn to_css_class(&self) -> &'static str {
        match self {
            AdapterHeat::Hot => "bg-status-error hover:brightness-110",
            AdapterHeat::Warm => "bg-status-warning hover:brightness-110",
            AdapterHeat::Cold => "bg-status-info hover:brightness-110",
            AdapterHeat::Inactive => "bg-muted-foreground/50 hover:bg-muted-foreground/60",
        }
    }

    /// Get emoji indicator for heat level
    pub fn to_emoji(&self) -> &'static str {
        match self {
            AdapterHeat::Hot => "🔥",
            AdapterHeat::Warm => "♨️",
            AdapterHeat::Cold => "❄️",
            AdapterHeat::Inactive => "⚪",
        }
    }
}

/// Adapter bar component showing active adapters as colored magnets.
///
/// Click toggles pin state; info icon navigates to adapter detail.
#[component]
pub fn AdapterBar(
    /// Current adapter states
    #[prop(into)]
    adapters: Signal<Vec<AdapterMagnet>>,
    /// Callback when an adapter pin is toggled
    #[prop(into)]
    on_toggle_pin: Callback<String>,
) -> impl IntoView {
    view! {
        <div class="flex gap-3 p-4 border-b bg-gradient-to-r from-card/80 to-card/50 backdrop-blur-sm">
            <div class="flex items-center gap-2 text-sm font-medium text-muted-foreground">
                <svg
                    class="w-4 h-4"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                    xmlns="http://www.w3.org/2000/svg"
                >
                    <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M13 10V3L4 14h7v7l9-11h-7z"
                    ></path>
                </svg>
                "Active Adapters"
            </div>

            <div class="flex gap-2 flex-wrap items-center flex-1">
                {move || {
                    let adapter_list = adapters.try_get().unwrap_or_default();
                    if adapter_list.is_empty() {
                        view! {
                            <span class="text-xs text-muted-foreground italic">
                                "No adapters loaded"
                            </span>
                        }.into_any()
                    } else {
                        let on_pin = on_toggle_pin;
                        adapter_list
                            .iter()
                            .map(|adapter| {
                                let color_class = adapter.heat.to_css_class();
                                let emoji = adapter.heat.to_emoji();
                                let opacity = if adapter.is_active {
                                    "opacity-100 shadow-lg scale-105"
                                } else {
                                    "opacity-70 hover:opacity-90"
                                };
                                let animation = if adapter.is_active {
                                    "animate-pulse"
                                } else {
                                    ""
                                };
                                let pinned_class = if adapter.is_pinned {
                                    "ring-2 ring-primary/60"
                                } else {
                                    ""
                                };

                                let heat_label = match adapter.heat {
                                    AdapterHeat::Hot => "Hot",
                                    AdapterHeat::Warm => "Warm",
                                    AdapterHeat::Cold => "Cold",
                                    AdapterHeat::Inactive => "Inactive",
                                };

                                let pin_action = if adapter.is_pinned { "Unpin" } else { "Pin" };
                                let status_label = if adapter.is_pinned && adapter.is_active {
                                    "Pinned, Active"
                                } else if adapter.is_pinned {
                                    "Pinned"
                                } else if adapter.is_active {
                                    "Active"
                                } else {
                                    "Idle"
                                };

                                let title = format!(
                                    "Click to {} \u{2022} {} ({})",
                                    pin_action,
                                    heat_label,
                                    status_label,
                                );

                                let adapter_id = adapter.adapter_id.clone();
                                let adapter_id_toggle = adapter_id.clone();
                                let href = format!("/adapters/{}", adapter_id);

                                view! {
                                    <div class="flex items-center gap-0.5">
                                        <button
                                            type="button"
                                            class=format!(
                                                "{} {} {} {} rounded-full px-3 py-1.5 text-xs font-medium text-white shadow-md transition-all duration-300 flex items-center gap-1 hover:brightness-110 cursor-pointer border-0",
                                                color_class,
                                                opacity,
                                                animation,
                                                pinned_class
                                            )
                                            title=title
                                            aria-label=format!("{} adapter {}", pin_action, adapter_id)
                                            aria-pressed=adapter.is_pinned.to_string()
                                            on:click=move |_| on_pin.run(adapter_id_toggle.clone())
                                        >
                                            <svg class="w-3 h-3 flex-shrink-0" viewBox="0 0 20 20" aria-hidden="true"
                                                fill=if adapter.is_pinned { "currentColor" } else { "none" }
                                                stroke=if adapter.is_pinned { "none" } else { "currentColor" }
                                                stroke-width=if adapter.is_pinned { "0" } else { "1.5" }
                                            >
                                                <path d="M5 5a2 2 0 012-2h6a2 2 0 012 2v2h2a1 1 0 011 1v1a1 1 0 01-1 1h-1v5a3 3 0 01-3 3H7a3 3 0 01-3-3v-5H3a1 1 0 01-1-1V8a1 1 0 011-1h2V5z"/>
                                            </svg>
                                            <span class="text-xs">{emoji}</span>
                                            <span class="font-mono">{adapter_id.clone()}</span>
                                        </button>
                                        <a
                                            href=href
                                            class="adapter-magnet-info"
                                            title=format!("View adapter {} details", adapter_id)
                                            aria-label=format!("View details for adapter {}", adapter_id)
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" aria-hidden="true">
                                                <path stroke-linecap="round" stroke-linejoin="round" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                                            </svg>
                                        </a>
                                    </div>
                                }
                            })
                            .collect::<Vec<_>>()
                            .into_any()
                    }
                }}
            </div>

            // Legend (compact)
            <div class="hidden md:flex items-center gap-3 text-2xs text-muted-foreground border-l pl-3">
                <div class="flex items-center gap-1">
                    <div class="w-2 h-2 rounded-full bg-status-error"></div>
                    "Hot"
                </div>
                <div class="flex items-center gap-1">
                    <div class="w-2 h-2 rounded-full bg-status-warning"></div>
                    "Warm"
                </div>
                <div class="flex items-center gap-1">
                    <div class="w-2 h-2 rounded-full bg-status-info"></div>
                    "Cold"
                </div>
            </div>
        </div>
    }
}

/// Suggested adapter from router preview
#[derive(Clone, Debug, PartialEq)]
pub struct SuggestedAdapterView {
    pub adapter_id: String,
    /// Display name for the adapter (falls back to adapter_id)
    pub display_name: String,
    pub confidence: f32,
    pub is_pinned: bool,
    /// Selected for next message (one-shot override)
    pub is_selected: bool,
    /// Optional: disabled state with reason
    pub disabled_reason: Option<String>,
    /// Optional: adapter description for tooltip
    pub description: Option<String>,
    /// Optional: languages/frameworks this adapter specializes in
    pub tags: Option<Vec<String>>,
}

impl SuggestedAdapterView {
    /// Get the chip state based on current properties
    pub fn chip_state(&self) -> AdapterChipState {
        if self.disabled_reason.is_some() {
            AdapterChipState::Disabled
        } else if self.is_pinned {
            AdapterChipState::Pinned
        } else if self.is_selected {
            AdapterChipState::Selected
        } else {
            AdapterChipState::Suggested
        }
    }
}

/// Deterministic sort comparator for adapter chips.
///
/// Sort order (stable):
/// 1. Primary: confidence/relevance score DESC (higher scores first)
/// 2. Secondary: adapter_id ASC (alphabetical for tie-breaking)
fn sort_adapters_deterministic(a: &SuggestedAdapterView, b: &SuggestedAdapterView) -> Ordering {
    // Primary: confidence DESC
    let confidence_cmp = b
        .confidence
        .partial_cmp(&a.confidence)
        .unwrap_or(Ordering::Equal);

    if confidence_cmp != Ordering::Equal {
        return confidence_cmp;
    }

    // Secondary: adapter_id ASC (stable tie-breaker)
    a.adapter_id.cmp(&b.adapter_id)
}

/// Suggested adapters bar with click-to-pin functionality
///
/// ## Liquid Glass Compliance
///
/// Uses Tier 1 glass background with proper borders and noise.
/// Animations are state-change-only (no idle movement).
///
/// ## Deterministic Layout
///
/// Chips are sorted by confidence DESC, then adapter_id ASC for stable ordering.
#[component]
pub fn SuggestedAdaptersBar(
    /// Suggested adapters from router preview
    #[prop(into)]
    suggestions: Signal<Vec<SuggestedAdapterView>>,
    /// Callback when an adapter is clicked (for one-shot override)
    #[prop(into)]
    on_select_override: Callback<String>,
    /// Callback when an adapter pin is toggled
    #[prop(into)]
    on_toggle_pin: Callback<String>,
    /// Whether suggestions are currently loading (streaming)
    #[prop(into)]
    loading: Signal<bool>,
) -> impl IntoView {
    // Expanded adapter tooltip state
    let expanded_adapter = RwSignal::new(Option::<String>::None);

    view! {
        <div
            class="adapter-magnet-bar"
            data-elevation="1"
            role="region"
            aria-label="Suggested adapters"
        >
            <div class="adapter-magnet-label">
                <svg
                    class="w-3.5 h-3.5"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                    xmlns="http://www.w3.org/2000/svg"
                    aria-hidden="true"
                >
                    <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"
                    ></path>
                </svg>
                <span>"Adapter Tray"</span>
            </div>

            <div class="adapter-magnet-chips" role="listbox" aria-label="Adapter suggestions">
                {move || {
                    let mut suggestion_list = suggestions.try_get().unwrap_or_default();
                    if suggestion_list.is_empty() {
                        let is_loading = loading.try_get().unwrap_or(false);
                        view! {
                            <span class="adapter-magnet-empty">
                                {if is_loading { "Routing adapters..." } else { "No suggestions yet" }}
                            </span>
                        }.into_any()
                    } else {
                        // Sort deterministically: confidence DESC, adapter_id ASC
                        suggestion_list.sort_by(sort_adapters_deterministic);
                        let max_items = 5usize;
                        if suggestion_list.len() > max_items {
                            suggestion_list.truncate(max_items);
                        }

                        suggestion_list
                            .into_iter()
                            .map(|adapter| {
                                let adapter_id = adapter.adapter_id.clone();
                                let adapter_id_click = adapter_id.clone();
                                let adapter_id_hover = adapter_id.clone();
                                let adapter_id_display = adapter_id.clone();
                                let adapter_name = adapter.display_name.clone();
                                let confidence = adapter.confidence;
                                let chip_state = adapter.chip_state();
                                let is_disabled = chip_state == AdapterChipState::Disabled;
                                let is_pinned = chip_state == AdapterChipState::Pinned;
                                let is_selected = chip_state == AdapterChipState::Selected;
                                let disabled_reason = adapter.disabled_reason.clone();
                                let description = adapter.description.clone();
                                let tags = adapter.tags.clone();
                                let on_toggle = on_toggle_pin;
                                let on_select = on_select_override;

                                let confidence_pct = (confidence * 100.0) as u32;
                                let confidence_label = confidence_to_label(confidence);

                                // Build tooltip content
                                let tooltip_text = if let Some(reason) = &disabled_reason {
                                    format!("{} (disabled: {})", adapter_id, reason)
                                } else {
                                    let mut tip = format!("{} - {} confidence", adapter_id, confidence_label);
                                    if is_pinned {
                                        tip.push_str(" (pinned)");
                                    } else if is_selected {
                                        tip.push_str(" (next message)");
                                    }
                                    tip
                                };

                                let aria_label = if is_disabled {
                                    format!(
                                        "Adapter {} is disabled: {}",
                                        adapter_id,
                                        disabled_reason.as_deref().unwrap_or("unavailable")
                                    )
                                } else {
                                    format!(
                                        "Use adapter {} for next message ({} confidence)",
                                        adapter_id,
                                        confidence_label
                                    )
                                };

                                view! {
                                    <div class="adapter-chip-wrapper flex items-center gap-1">
                                        <button
                                            type="button"
                                            class=format!(
                                                "adapter-chip {} {}",
                                                chip_state.to_css_class(),
                                                confidence_to_css_class(confidence)
                                            )
                                            title=tooltip_text.clone()
                                            disabled=is_disabled
                                            on:click=move |_| {
                                                if !is_disabled {
                                                    on_select.run(adapter_id_click.clone());
                                                }
                                            }
                                            on:mouseenter=move |_| {
                                                expanded_adapter.set(Some(adapter_id_hover.clone()));
                                            }
                                            on:mouseleave=move |_| {
                                                expanded_adapter.set(None);
                                            }
                                            aria-pressed=is_selected.to_string()
                                            aria-label=aria_label
                                            aria-disabled=is_disabled.to_string()
                                            role="option"
                                            aria-selected=is_selected.to_string()
                                        >
                                            // Pin icon (state indicator)
                                            <AdapterChipIcon state=chip_state/>
                                            <div class="flex flex-col text-left leading-tight min-w-[7rem]">
                                                <span class="text-[10px] font-semibold">{adapter_name}</span>
                                                <span class="text-[9px] text-white/80">
                                                    {description.clone().unwrap_or_else(|| "Unknown purpose".to_string())}
                                                </span>
                                            </div>
                                            <div class="h-1 w-12 rounded-full bg-white/20 overflow-hidden">
                                                <div
                                                    class="h-full bg-white/70"
                                                    style=format!("width: {}%;", confidence_pct)
                                                ></div>
                                            </div>
                                        </button>

                                        <button
                                            type="button"
                                            class=move || {
                                                if is_pinned {
                                                    "h-7 w-7 inline-flex items-center justify-center rounded-full border border-border bg-background/80 text-primary shadow-sm hover:bg-muted transition-colors text-[10px]"
                                                } else {
                                                    "h-7 w-7 inline-flex items-center justify-center rounded-full border border-border bg-background/80 text-muted-foreground hover:text-foreground hover:bg-muted transition-colors text-[10px]"
                                                }
                                            }
                                            title=if is_pinned { "Unpin adapter" } else { "Pin adapter" }
                                            aria-label=if is_pinned { "Unpin adapter" } else { "Pin adapter" }
                                            on:click=move |_| {
                                                on_toggle.run(adapter_id_display.clone());
                                            }
                                        >
                                            <svg
                                                xmlns="http://www.w3.org/2000/svg"
                                                class="h-3.5 w-3.5"
                                                viewBox="0 0 20 20"
                                                fill="currentColor"
                                                aria-hidden="true"
                                            >
                                                <path d="M5 5a2 2 0 012-2h6a2 2 0 012 2v2h2a1 1 0 011 1v1a1 1 0 01-1 1h-1v5a3 3 0 01-3 3H7a3 3 0 01-3-3v-5H3a1 1 0 01-1-1V8a1 1 0 011-1h2V5z"/>
                                            </svg>
                                        </button>

                                        // Expanded tooltip on hover (only for non-disabled)
                                        {move || {
                                            let is_expanded = expanded_adapter.try_get().flatten() == Some(adapter_id.clone());
                                            let desc = description.clone();
                                            let tag_list = tags.clone();

                                            if is_expanded && !is_disabled && (desc.is_some() || tag_list.is_some()) {
                                                Some(view! {
                                                    <AdapterChipTooltip
                                                        adapter_id=adapter_id.clone()
                                                        description=desc
                                                        tags=tag_list
                                                        confidence=confidence
                                                    />
                                                })
                                            } else {
                                                None
                                            }
                                        }}
                                    </div>
                                }
                            })
                            .collect::<Vec<_>>()
                            .into_any()
                    }
                }}
            </div>
        </div>
    }
}

/// CSS class based on confidence level
fn confidence_to_css_class(confidence: f32) -> &'static str {
    if confidence > 0.7 {
        "adapter-chip-high"
    } else if confidence > 0.4 {
        "adapter-chip-medium"
    } else {
        "adapter-chip-low"
    }
}

/// Human-readable confidence label (no numeric display)
fn confidence_to_label(confidence: f32) -> &'static str {
    if confidence > 0.7 {
        "High"
    } else if confidence > 0.4 {
        "Medium"
    } else {
        "Low"
    }
}

/// Icon for adapter chip based on state
#[component]
fn AdapterChipIcon(state: AdapterChipState) -> impl IntoView {
    match state {
        AdapterChipState::Pinned => view! {
            <svg class="adapter-chip-icon" fill="currentColor" viewBox="0 0 20 20" aria-hidden="true">
                <path d="M5 5a2 2 0 012-2h6a2 2 0 012 2v2h2a1 1 0 011 1v1a1 1 0 01-1 1h-1v5a3 3 0 01-3 3H7a3 3 0 01-3-3v-5H3a1 1 0 01-1-1V8a1 1 0 011-1h2V5z"/>
            </svg>
        }
        .into_any(),
        AdapterChipState::Selected => view! {
            <svg class="adapter-chip-icon" fill="currentColor" viewBox="0 0 20 20" aria-hidden="true">
                <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd"/>
            </svg>
        }
        .into_any(),
        AdapterChipState::Disabled => view! {
            <svg class="adapter-chip-icon" fill="none" stroke="currentColor" viewBox="0 0 20 20" stroke-width="1.5" aria-hidden="true">
                <path stroke-linecap="round" stroke-linejoin="round" d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636"/>
            </svg>
        }
        .into_any(),
        AdapterChipState::Suggested => view! {
            <svg class="adapter-chip-icon adapter-chip-icon-faded" fill="none" stroke="currentColor" viewBox="0 0 20 20" stroke-width="1.5" aria-hidden="true">
                <path stroke-linecap="round" stroke-linejoin="round" d="M5 5a2 2 0 012-2h6a2 2 0 012 2v2h2a1 1 0 011 1v1a1 1 0 01-1 1h-1v5a3 3 0 01-3 3H7a3 3 0 01-3-3v-5H3a1 1 0 01-1-1V8a1 1 0 011-1h2V5z"/>
            </svg>
        }
        .into_any(),
    }
}

/// Expanded tooltip showing adapter details
#[component]
fn AdapterChipTooltip(
    adapter_id: String,
    description: Option<String>,
    tags: Option<Vec<String>>,
    confidence: f32,
) -> impl IntoView {
    let aria_label = format!("Details for adapter {}", adapter_id);
    let confidence_label = confidence_to_label(confidence);
    view! {
        <div
            class="adapter-chip-tooltip"
            role="tooltip"
            aria-label=aria_label
        >
            <div class="adapter-chip-tooltip-header">
                <span class="adapter-chip-tooltip-id">{adapter_id}</span>
                <span class="adapter-chip-tooltip-confidence">
                    {confidence_label}
                </span>
            </div>

            {description.map(|desc| view! {
                <p class="adapter-chip-tooltip-desc">{desc}</p>
            })}

            {tags.map(|tag_list| view! {
                <div class="adapter-chip-tooltip-tags">
                    {tag_list.into_iter().map(|tag| view! {
                        <span class="adapter-chip-tooltip-tag">{tag}</span>
                    }).collect::<Vec<_>>()}
                </div>
            })}
        </div>
    }
}

/// Unified adapters region for the chat session view.
///
/// Combines Active, Pinned, and Suggested adapter sections into a single region
/// with a pending indicator and "Manage" button for adapter selection.
#[component]
pub fn ChatAdaptersRegion(
    /// Active adapters (from SSE AdapterStateUpdate)
    #[prop(into)]
    active_adapters: Signal<Vec<AdapterMagnet>>,
    /// Pinned adapter IDs (user intent)
    #[prop(into)]
    pinned_adapters: Signal<Vec<String>>,
    /// Suggested adapters from router preview
    #[prop(into)]
    suggestions: Signal<Vec<SuggestedAdapterView>>,
    /// Whether adapter selection is pending SSE confirmation
    #[prop(into)]
    pending: Signal<bool>,
    /// Callback when an adapter is clicked for one-shot override
    #[prop(into)]
    on_select_override: Callback<String>,
    /// Callback when an adapter pin is toggled
    #[prop(into)]
    on_toggle_pin: Callback<String>,
    /// Callback to set the full pinned adapter list (from manage dialog)
    #[prop(into)]
    on_set_pinned: Callback<Vec<String>>,
    /// Whether suggestions are loading
    #[prop(into)]
    loading: Signal<bool>,
) -> impl IntoView {
    let show_manage = RwSignal::new(false);

    // Derive pinned-only magnets (pinned but not in active set)
    let pinned_only_magnets = Memo::new(move |_| {
        let active = active_adapters.try_get().unwrap_or_default();
        let pinned = pinned_adapters.try_get().unwrap_or_default();
        let active_ids: std::collections::HashSet<_> =
            active.iter().map(|a| a.adapter_id.as_str()).collect();
        pinned
            .iter()
            .filter(|id| !active_ids.contains(id.as_str()))
            .map(|id| AdapterMagnet {
                adapter_id: id.clone(),
                heat: AdapterHeat::Inactive,
                is_active: false,
                is_pinned: true,
            })
            .collect::<Vec<_>>()
    });

    // Mark active magnets that are also pinned
    let active_with_pins = Memo::new(move |_| {
        let active = active_adapters.try_get().unwrap_or_default();
        let pinned = pinned_adapters.try_get().unwrap_or_default();
        active
            .into_iter()
            .map(|mut m| {
                m.is_pinned = pinned.contains(&m.adapter_id);
                m
            })
            .collect::<Vec<_>>()
    });

    let has_active = Memo::new(move |_| !active_with_pins.try_get().unwrap_or_default().is_empty());
    let has_pinned_only = Memo::new(move |_| !pinned_only_magnets.try_get().unwrap_or_default().is_empty());

    view! {
        <div
            class="chat-adapters-region"
            role="region"
            aria-label="Adapters"
        >
            // Header with Manage button
            <div class="chat-adapters-header">
                <div class="flex items-center gap-2 text-sm font-medium text-muted-foreground">
                    <svg
                        class="w-4 h-4"
                        fill="none"
                        stroke="currentColor"
                        viewBox="0 0 24 24"
                        xmlns="http://www.w3.org/2000/svg"
                        aria-hidden="true"
                    >
                        <path
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            stroke-width="2"
                            d="M13 10V3L4 14h7v7l9-11h-7z"
                        ></path>
                    </svg>
                    "Adapters"
                </div>
                <div class="flex items-center gap-2">
                    {move || pending.try_get().unwrap_or(false).then(|| view! {
                        <span
                            class="chat-adapters-pending-badge"
                            role="status"
                            aria-label="Adapter changes pending confirmation"
                        >
                            "Pending next message"
                        </span>
                    })}
                    <button
                        type="button"
                        class="btn btn-outline btn-xs"
                        on:click=move |_| show_manage.set(true)
                        aria-label="Manage adapter selection"
                    >
                        "Manage"
                    </button>
                </div>
            </div>

            // Active section
            {move || has_active.try_get().unwrap_or(false).then(|| {
                let magnets = active_with_pins.try_get().unwrap_or_default();
                let on_pin = on_toggle_pin;
                view! {
                    <div class="chat-adapters-section">
                        <span class="chat-adapters-section-label">"Active"</span>
                        <div class="flex gap-2 flex-wrap items-center">
                            {magnets
                                .iter()
                                .map(|adapter| {
                                    let color_class = adapter.heat.to_css_class();
                                    let emoji = adapter.heat.to_emoji();
                                    let opacity = if adapter.is_active {
                                        "opacity-100 shadow-lg scale-105"
                                    } else {
                                        "opacity-70 hover:opacity-90"
                                    };
                                    let animation = if adapter.is_active { "animate-pulse" } else { "" };
                                    let pinned_class = if adapter.is_pinned { "ring-2 ring-primary/60" } else { "" };
                                    let heat_label = match adapter.heat {
                                        AdapterHeat::Hot => "Hot",
                                        AdapterHeat::Warm => "Warm",
                                        AdapterHeat::Cold => "Cold",
                                        AdapterHeat::Inactive => "Inactive",
                                    };
                                    let pin_action = if adapter.is_pinned { "Unpin" } else { "Pin" };
                                    let title = format!("Click to {} \u{2022} {} (Active{})", pin_action, heat_label, if adapter.is_pinned { ", Pinned" } else { "" });
                                    let adapter_id = adapter.adapter_id.clone();
                                    let adapter_id_toggle = adapter_id.clone();
                                    let href = format!("/adapters/{}", adapter_id);
                                    view! {
                                        <div class="flex items-center gap-0.5">
                                            <button
                                                type="button"
                                                class=format!(
                                                    "{} {} {} {} rounded-full px-3 py-1.5 text-xs font-medium text-white shadow-md transition-all duration-300 flex items-center gap-1 hover:brightness-110 cursor-pointer border-0",
                                                    color_class, opacity, animation, pinned_class
                                                )
                                                title=title
                                                aria-label=format!("{} adapter {}", pin_action, adapter_id)
                                                aria-pressed=adapter.is_pinned.to_string()
                                                on:click=move |_| on_pin.run(adapter_id_toggle.clone())
                                            >
                                                // Pin/unpin icon
                                                <svg class="w-3 h-3 flex-shrink-0" viewBox="0 0 20 20" aria-hidden="true"
                                                    fill=if adapter.is_pinned { "currentColor" } else { "none" }
                                                    stroke=if adapter.is_pinned { "none" } else { "currentColor" }
                                                    stroke-width=if adapter.is_pinned { "0" } else { "1.5" }
                                                >
                                                    <path d="M5 5a2 2 0 012-2h6a2 2 0 012 2v2h2a1 1 0 011 1v1a1 1 0 01-1 1h-1v5a3 3 0 01-3 3H7a3 3 0 01-3-3v-5H3a1 1 0 01-1-1V8a1 1 0 011-1h2V5z"/>
                                                </svg>
                                                <span class="text-xs">{emoji}</span>
                                                <span class="font-mono">{adapter_id.clone()}</span>
                                            </button>
                                            // Info link to adapter detail page
                                            <a
                                                href=href
                                                class="adapter-magnet-info"
                                                title=format!("View adapter {} details", adapter_id)
                                                aria-label=format!("View details for adapter {}", adapter_id)
                                            >
                                                <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" aria-hidden="true">
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                                                </svg>
                                            </a>
                                        </div>
                                    }
                                })
                                .collect::<Vec<_>>()
                            }
                        </div>
                    </div>
                }
            })}

            // Pinned-only section (pinned adapters not yet in active set)
            {move || has_pinned_only.try_get().unwrap_or(false).then(|| {
                let magnets = pinned_only_magnets.try_get().unwrap_or_default();
                let on_unpin = on_toggle_pin;
                view! {
                    <div class="chat-adapters-section">
                        <span class="chat-adapters-section-label">"Pinned"</span>
                        <div class="flex gap-2 flex-wrap items-center">
                            {magnets
                                .iter()
                                .map(|adapter| {
                                    let adapter_id = adapter.adapter_id.clone();
                                    let adapter_id_toggle = adapter_id.clone();
                                    let href = format!("/adapters/{}", adapter_id);
                                    view! {
                                        <div class="flex items-center gap-0.5">
                                            <button
                                                type="button"
                                                class="bg-primary/80 ring-2 ring-primary/60 rounded-full px-3 py-1.5 text-xs font-medium text-white shadow-md transition-all duration-300 flex items-center gap-1 hover:brightness-110 opacity-70 cursor-pointer border-0"
                                                title=format!("Click to unpin {} (awaiting inference)", adapter_id)
                                                aria-label=format!("Unpin adapter {}", adapter_id)
                                                aria-pressed="true"
                                                on:click=move |_| on_unpin.run(adapter_id_toggle.clone())
                                            >
                                                <svg class="w-3 h-3 flex-shrink-0" fill="currentColor" viewBox="0 0 20 20" aria-hidden="true">
                                                    <path d="M5 5a2 2 0 012-2h6a2 2 0 012 2v2h2a1 1 0 011 1v1a1 1 0 01-1 1h-1v5a3 3 0 01-3 3H7a3 3 0 01-3-3v-5H3a1 1 0 01-1-1V8a1 1 0 011-1h2V5z"/>
                                                </svg>
                                                <span class="font-mono">{adapter_id.clone()}</span>
                                            </button>
                                            // Info link to adapter detail page
                                            <a
                                                href=href
                                                class="adapter-magnet-info"
                                                title=format!("View adapter {} details", adapter_id)
                                                aria-label=format!("View details for adapter {}", adapter_id)
                                            >
                                                <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" aria-hidden="true">
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                                                </svg>
                                            </a>
                                        </div>
                                    }
                                })
                                .collect::<Vec<_>>()
                            }
                        </div>
                    </div>
                }
            })}

            // Suggested section
            <div class="chat-adapters-section">
                <SuggestedAdaptersBar
                    suggestions=suggestions
                    on_select_override=on_select_override
                    on_toggle_pin=on_toggle_pin
                    loading=loading
                />
            </div>

            // Manage dialog
            <AdapterManageDialog
                open=show_manage
                pinned_adapters=pinned_adapters
                suggestions=suggestions
                active_adapters=active_adapters
                on_apply=on_set_pinned
            />
        </div>
    }
}

/// Dialog for managing adapter pin selection.
///
/// Shows a searchable list of known adapters (active + suggested) with
/// checkboxes. Apply sets the pinned set in state.
#[component]
pub fn AdapterManageDialog(
    /// Controls dialog visibility
    #[prop(into)]
    open: RwSignal<bool>,
    /// Current pinned adapter IDs
    #[prop(into)]
    pinned_adapters: Signal<Vec<String>>,
    /// Suggested adapters (for adapter discovery)
    #[prop(into)]
    suggestions: Signal<Vec<SuggestedAdapterView>>,
    /// Active adapters (for adapter discovery)
    #[prop(into)]
    active_adapters: Signal<Vec<AdapterMagnet>>,
    /// Callback with the new pinned set on Apply
    #[prop(into)]
    on_apply: Callback<Vec<String>>,
) -> impl IntoView {
    let search_query = RwSignal::new(String::new());
    // Local draft of selected adapter IDs (initialized from pinned when dialog opens)
    let draft_selection = RwSignal::new(Vec::<String>::new());

    // Sync draft from pinned when dialog opens
    Effect::new(move |_| {
        if open.try_get().unwrap_or(false) {
            draft_selection.set(pinned_adapters.get_untracked());
        }
    });

    // Build list of all known adapters (union of active + suggested), deduped
    let all_adapters = Memo::new(move |_| {
        let mut seen = std::collections::HashSet::new();
        let mut items = Vec::new();
        let query = search_query.try_get().unwrap_or_default().to_lowercase();

        for m in active_adapters.try_get().unwrap_or_default() {
            if seen.insert(m.adapter_id.clone())
                && (query.is_empty() || m.adapter_id.to_lowercase().contains(&query))
            {
                items.push((m.adapter_id.clone(), None::<String>));
            }
        }
        for s in suggestions.try_get().unwrap_or_default() {
            if seen.insert(s.adapter_id.clone())
                && (query.is_empty()
                    || s.adapter_id.to_lowercase().contains(&query)
                    || s.display_name.to_lowercase().contains(&query))
            {
                items.push((s.adapter_id.clone(), Some(s.display_name.clone())));
            }
        }
        items.sort_by(|a, b| a.0.cmp(&b.0));
        items
    });

    let do_apply = move |_: web_sys::SubmitEvent| {
        on_apply.run(draft_selection.get_untracked());
        open.set(false);
    };

    view! {
        <crate::components::Dialog
            open=open
            title="Manage Adapters".to_string()
            description="Select adapters to pin for upcoming messages.".to_string()
            size=crate::components::DialogSize::Md
            scrollable=true
        >
            <form on:submit=do_apply class="space-y-4">
                <div class="relative">
                    <input
                        type="text"
                        class="input input-sm w-full"
                        placeholder="Search adapters..."
                        aria-label="Search adapters"
                        on:input=move |ev| {
                            use leptos::prelude::*;
                            search_query.set(event_target_value(&ev));
                        }
                        prop:value=move || search_query.try_get().unwrap_or_default()
                    />
                </div>
                <div class="space-y-1 max-h-64 overflow-y-auto">
                    {move || {
                        let items = all_adapters.try_get().unwrap_or_default();
                        let draft = draft_selection.try_get().unwrap_or_default();
                        if items.is_empty() {
                            view! {
                                <p class="text-xs text-muted-foreground italic py-2">
                                    "No adapters found"
                                </p>
                            }.into_any()
                        } else {
                            items
                                .into_iter()
                                .map(|(adapter_id, display_name)| {
                                    let id = adapter_id.clone();
                                    let id_toggle = adapter_id.clone();
                                    let checked = draft.contains(&adapter_id);
                                    let label = display_name.unwrap_or_else(|| adapter_id.clone());
                                    view! {
                                        <label class="flex items-center gap-2 px-2 py-1.5 rounded-md hover:bg-muted cursor-pointer text-sm">
                                            <input
                                                type="checkbox"
                                                class="accent-primary"
                                                prop:checked=checked
                                                on:change=move |_| {
                                                    draft_selection.update(|d| {
                                                        if let Some(pos) = d.iter().position(|x| x == &id_toggle) {
                                                            d.remove(pos);
                                                        } else {
                                                            d.push(id_toggle.clone());
                                                        }
                                                    });
                                                }
                                            />
                                            <span class="font-mono text-xs">{id}</span>
                                            <span class="text-xs text-muted-foreground ml-auto">{label}</span>
                                        </label>
                                    }
                                })
                                .collect::<Vec<_>>()
                                .into_any()
                        }
                    }}
                </div>
                <div class="flex justify-end gap-2 pt-2 border-t border-border">
                    <button
                        type="button"
                        class="btn btn-outline btn-sm"
                        on:click=move |_| open.set(false)
                    >
                        "Cancel"
                    </button>
                    <button
                        type="submit"
                        class="btn btn-primary btn-sm"
                    >
                        "Apply"
                    </button>
                </div>
            </form>
        </crate::components::Dialog>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heat_classification() {
        assert_eq!(AdapterHeat::Hot.to_emoji(), "🔥");
        assert_eq!(AdapterHeat::Warm.to_emoji(), "♨️");
        assert_eq!(AdapterHeat::Cold.to_emoji(), "❄️");
    }

    #[test]
    fn test_css_classes() {
        assert!(AdapterHeat::Hot.to_css_class().contains("status-error"));
        assert!(AdapterHeat::Warm.to_css_class().contains("status-warning"));
        assert!(AdapterHeat::Cold.to_css_class().contains("status-info"));
    }

    #[test]
    fn test_chip_state_css_classes() {
        assert_eq!(
            AdapterChipState::Suggested.to_css_class(),
            "adapter-chip-suggested"
        );
        assert_eq!(
            AdapterChipState::Selected.to_css_class(),
            "adapter-chip-selected"
        );
        assert_eq!(
            AdapterChipState::Pinned.to_css_class(),
            "adapter-chip-pinned"
        );
        assert_eq!(
            AdapterChipState::Disabled.to_css_class(),
            "adapter-chip-disabled"
        );
    }

    #[test]
    fn test_chip_state_labels() {
        assert_eq!(AdapterChipState::Suggested.to_label(), "suggested");
        assert_eq!(AdapterChipState::Selected.to_label(), "selected");
        assert_eq!(AdapterChipState::Pinned.to_label(), "pinned");
        assert_eq!(AdapterChipState::Disabled.to_label(), "disabled");
    }

    #[test]
    fn test_suggested_adapter_view_chip_state() {
        // Disabled takes precedence
        let disabled = SuggestedAdapterView {
            adapter_id: "test".to_string(),
            display_name: "test".to_string(),
            confidence: 0.9,
            is_pinned: true, // Even if pinned, disabled wins
            is_selected: false,
            disabled_reason: Some("rate limited".to_string()),
            description: None,
            tags: None,
        };
        assert_eq!(disabled.chip_state(), AdapterChipState::Disabled);

        // Pinned state
        let pinned = SuggestedAdapterView {
            adapter_id: "test".to_string(),
            display_name: "test".to_string(),
            confidence: 0.8,
            is_pinned: true,
            is_selected: false,
            disabled_reason: None,
            description: None,
            tags: None,
        };
        assert_eq!(pinned.chip_state(), AdapterChipState::Pinned);

        // Selected state
        let selected = SuggestedAdapterView {
            adapter_id: "test".to_string(),
            display_name: "test".to_string(),
            confidence: 0.7,
            is_pinned: false,
            is_selected: true,
            disabled_reason: None,
            description: None,
            tags: None,
        };
        assert_eq!(selected.chip_state(), AdapterChipState::Selected);

        // Default to suggested
        let suggested = SuggestedAdapterView {
            adapter_id: "test".to_string(),
            display_name: "test".to_string(),
            confidence: 0.5,
            is_pinned: false,
            is_selected: false,
            disabled_reason: None,
            description: None,
            tags: None,
        };
        assert_eq!(suggested.chip_state(), AdapterChipState::Suggested);
    }

    #[test]
    fn test_deterministic_sort_by_confidence_desc() {
        let mut adapters: Vec<_> = [
            SuggestedAdapterView {
                adapter_id: "low".to_string(),
                display_name: "low".to_string(),
                confidence: 0.3,
                is_pinned: false,
                is_selected: false,
                disabled_reason: None,
                description: None,
                tags: None,
            },
            SuggestedAdapterView {
                adapter_id: "high".to_string(),
                display_name: "high".to_string(),
                confidence: 0.9,
                is_pinned: false,
                is_selected: false,
                disabled_reason: None,
                description: None,
                tags: None,
            },
            SuggestedAdapterView {
                adapter_id: "medium".to_string(),
                display_name: "medium".to_string(),
                confidence: 0.6,
                is_pinned: false,
                is_selected: false,
                disabled_reason: None,
                description: None,
                tags: None,
            },
        ]
        .into();

        adapters.sort_by(sort_adapters_deterministic);

        // Should be sorted by confidence DESC
        assert_eq!(adapters[0].adapter_id, "high");
        assert_eq!(adapters[1].adapter_id, "medium");
        assert_eq!(adapters[2].adapter_id, "low");
    }

    #[test]
    fn test_deterministic_sort_tie_breaker_by_id_asc() {
        let mut adapters: Vec<_> = [
            SuggestedAdapterView {
                adapter_id: "zebra".to_string(),
                display_name: "zebra".to_string(),
                confidence: 0.5,
                is_pinned: false,
                is_selected: false,
                disabled_reason: None,
                description: None,
                tags: None,
            },
            SuggestedAdapterView {
                adapter_id: "alpha".to_string(),
                display_name: "alpha".to_string(),
                confidence: 0.5, // Same confidence
                is_pinned: false,
                is_selected: false,
                disabled_reason: None,
                description: None,
                tags: None,
            },
            SuggestedAdapterView {
                adapter_id: "beta".to_string(),
                display_name: "beta".to_string(),
                confidence: 0.5, // Same confidence
                is_pinned: false,
                is_selected: false,
                disabled_reason: None,
                description: None,
                tags: None,
            },
        ]
        .into();

        adapters.sort_by(sort_adapters_deterministic);

        // Equal confidence should sort by adapter_id ASC (alphabetical)
        assert_eq!(adapters[0].adapter_id, "alpha");
        assert_eq!(adapters[1].adapter_id, "beta");
        assert_eq!(adapters[2].adapter_id, "zebra");
    }

    #[test]
    fn test_confidence_css_class() {
        assert_eq!(confidence_to_css_class(0.8), "adapter-chip-high");
        assert_eq!(confidence_to_css_class(0.71), "adapter-chip-high");
        assert_eq!(confidence_to_css_class(0.7), "adapter-chip-medium");
        assert_eq!(confidence_to_css_class(0.5), "adapter-chip-medium");
        assert_eq!(confidence_to_css_class(0.41), "adapter-chip-medium");
        assert_eq!(confidence_to_css_class(0.4), "adapter-chip-low");
        assert_eq!(confidence_to_css_class(0.2), "adapter-chip-low");
        assert_eq!(confidence_to_css_class(0.0), "adapter-chip-low");
    }
}
