//! Adapter visualization bar component
//!
//! Displays active adapters as "magnets" with color-coded heat levels:
//! - Hot (red): >10 uses per minute
//! - Warm (orange): 1-10 uses per minute  
//! - Cold (blue): <1 use per minute
//! - Active (glowing): Currently executing inference

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Adapter state for visualization
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    /// Get CSS classes for this heat level
    pub fn to_css_class(&self) -> &'static str {
        match self {
            AdapterHeat::Hot => "bg-red-500 hover:bg-red-600",
            AdapterHeat::Warm => "bg-orange-500 hover:bg-orange-600",
            AdapterHeat::Cold => "bg-blue-500 hover:bg-blue-600",
            AdapterHeat::Inactive => "bg-gray-500 hover:bg-gray-600",
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
    adapters: ReadSignal<Vec<AdapterMagnet>>,
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
                    <div class="w-2 h-2 rounded-full bg-red-500"></div>
                    "Hot"
                </div>
                <div class="flex items-center gap-1">
                    <div class="w-2 h-2 rounded-full bg-orange-500"></div>
                    "Warm"
                </div>
                <div class="flex items-center gap-1">
                    <div class="w-2 h-2 rounded-full bg-blue-500"></div>
                    "Cold"
                </div>
            </div>
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
        assert!(AdapterHeat::Hot.to_css_class().contains("red"));
        assert!(AdapterHeat::Warm.to_css_class().contains("orange"));
        assert!(AdapterHeat::Cold.to_css_class().contains("blue"));
    }
}
