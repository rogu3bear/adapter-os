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

/// Adapter bar component showing active adapters as colored magnets
#[component]
pub fn AdapterBar(
    /// Current adapter states
    #[prop(into)]
    adapters: Signal<Vec<AdapterMagnet>>,
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
                    let adapter_list = adapters.get();
                    if adapter_list.is_empty() {
                        view! {
                            <span class="text-xs text-muted-foreground italic">
                                "No adapters loaded"
                            </span>
                        }.into_any()
                    } else {
                        adapter_list
                            .iter()
                            .map(|adapter| {
                                let color_class = adapter.heat.to_css_class();
                                let emoji = adapter.heat.to_emoji();
                                let opacity = if adapter.is_active {
                                    "opacity-100 shadow-lg ring-2 ring-white/50 scale-105"
                                } else {
                                    "opacity-70 hover:opacity-90"
                                };
                                let animation = if adapter.is_active {
                                    "animate-pulse"
                                } else {
                                    ""
                                };

                                let heat_label = match adapter.heat {
                                    AdapterHeat::Hot => "Hot",
                                    AdapterHeat::Warm => "Warm",
                                    AdapterHeat::Cold => "Cold",
                                    AdapterHeat::Inactive => "Inactive",
                                };

                                let title = format!(
                                    "{} - {} ({})",
                                    adapter.adapter_id,
                                    heat_label,
                                    if adapter.is_active { "Active" } else { "Idle" }
                                );

                                view! {
                                    <div
                                        class=format!(
                                            "{} {} {} rounded-full px-3 py-1.5 text-xs font-medium text-white shadow-md transition-all duration-300 cursor-default flex items-center gap-1",
                                            color_class,
                                            opacity,
                                            animation
                                        )
                                        title=title
                                        role="status"
                                        aria-label=format!("Adapter {} is {}", adapter.adapter_id, heat_label)
                                    >
                                        <span class="text-xs">{emoji}</span>
                                        <span class="font-mono">{adapter.adapter_id.clone()}</span>
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
    pub confidence: f32,
    pub is_pinned: bool,
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
    /// Callback when an adapter is clicked (for pinning)
    #[prop(into)]
    on_toggle_pin: Callback<String>,
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
                <span>"Suggested"</span>
            </div>

            <div class="adapter-magnet-chips" role="listbox" aria-label="Adapter suggestions">
                {move || {
                    let mut suggestion_list = suggestions.get();
                    if suggestion_list.is_empty() {
                        view! {
                            <span class="adapter-magnet-empty">
                                "Type to see adapter suggestions..."
                            </span>
                        }.into_any()
                    } else {
                        // Sort deterministically: confidence DESC, adapter_id ASC
                        suggestion_list.sort_by(sort_adapters_deterministic);

                        suggestion_list
                            .into_iter()
                            .map(|adapter| {
                                let adapter_id = adapter.adapter_id.clone();
                                let adapter_id_click = adapter_id.clone();
                                let adapter_id_hover = adapter_id.clone();
                                let adapter_id_display = adapter_id.clone();
                                let confidence = adapter.confidence;
                                let chip_state = adapter.chip_state();
                                let is_disabled = chip_state == AdapterChipState::Disabled;
                                let is_pinned = chip_state == AdapterChipState::Pinned;
                                let disabled_reason = adapter.disabled_reason.clone();
                                let description = adapter.description.clone();
                                let tags = adapter.tags.clone();
                                let on_toggle = on_toggle_pin.clone();

                                let confidence_pct = (confidence * 100.0) as u32;

                                // Build tooltip content
                                let tooltip_text = if let Some(reason) = &disabled_reason {
                                    format!("{} (disabled: {})", adapter_id, reason)
                                } else {
                                    let mut tip = format!("{} - {}% confidence", adapter_id, confidence_pct);
                                    if is_pinned {
                                        tip.push_str(" (pinned)");
                                    } else {
                                        tip.push_str(" - click to pin");
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
                                        "{} adapter {} with {}% confidence",
                                        if is_pinned { "Unpin" } else { "Pin" },
                                        adapter_id,
                                        confidence_pct
                                    )
                                };

                                view! {
                                    <div class="adapter-chip-wrapper">
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
                                                    on_toggle.run(adapter_id_click.clone());
                                                }
                                            }
                                            on:mouseenter=move |_| {
                                                expanded_adapter.set(Some(adapter_id_hover.clone()));
                                            }
                                            on:mouseleave=move |_| {
                                                expanded_adapter.set(None);
                                            }
                                            aria-pressed=is_pinned.to_string()
                                            aria-label=aria_label
                                            aria-disabled=is_disabled.to_string()
                                            role="option"
                                            aria-selected=is_pinned.to_string()
                                        >
                                            // Pin icon (state indicator)
                                            <AdapterChipIcon state=chip_state/>
                                            <span class="adapter-chip-id">{adapter_id_display}</span>
                                            <span class="adapter-chip-confidence">{format!("{}%", confidence_pct)}</span>
                                        </button>

                                        // Expanded tooltip on hover (only for non-disabled)
                                        {move || {
                                            let is_expanded = expanded_adapter.get() == Some(adapter_id.clone());
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
    view! {
        <div
            class="adapter-chip-tooltip"
            role="tooltip"
            aria-label=aria_label
        >
            <div class="adapter-chip-tooltip-header">
                <span class="adapter-chip-tooltip-id">{adapter_id}</span>
                <span class="adapter-chip-tooltip-confidence">
                    {format!("{:.0}%", confidence * 100.0)}
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
            confidence: 0.9,
            is_pinned: true, // Even if pinned, disabled wins
            disabled_reason: Some("rate limited".to_string()),
            description: None,
            tags: None,
        };
        assert_eq!(disabled.chip_state(), AdapterChipState::Disabled);

        // Pinned state
        let pinned = SuggestedAdapterView {
            adapter_id: "test".to_string(),
            confidence: 0.8,
            is_pinned: true,
            disabled_reason: None,
            description: None,
            tags: None,
        };
        assert_eq!(pinned.chip_state(), AdapterChipState::Pinned);

        // Default to suggested
        let suggested = SuggestedAdapterView {
            adapter_id: "test".to_string(),
            confidence: 0.5,
            is_pinned: false,
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
                confidence: 0.3,
                is_pinned: false,
                disabled_reason: None,
                description: None,
                tags: None,
            },
            SuggestedAdapterView {
                adapter_id: "high".to_string(),
                confidence: 0.9,
                is_pinned: false,
                disabled_reason: None,
                description: None,
                tags: None,
            },
            SuggestedAdapterView {
                adapter_id: "medium".to_string(),
                confidence: 0.6,
                is_pinned: false,
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
                confidence: 0.5,
                is_pinned: false,
                disabled_reason: None,
                description: None,
                tags: None,
            },
            SuggestedAdapterView {
                adapter_id: "alpha".to_string(),
                confidence: 0.5, // Same confidence
                is_pinned: false,
                disabled_reason: None,
                description: None,
                tags: None,
            },
            SuggestedAdapterView {
                adapter_id: "beta".to_string(),
                confidence: 0.5, // Same confidence
                is_pinned: false,
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
