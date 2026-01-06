//! Accessible tooltip component for charts.

use leptos::prelude::*;

/// Tooltip anchor position relative to cursor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TooltipAnchor {
    /// Position tooltip above the point
    Top,
    /// Position tooltip below the point
    #[default]
    Bottom,
    /// Automatically choose based on position
    Auto,
}

/// Chart tooltip content.
#[derive(Debug, Clone, Default)]
pub struct TooltipContent {
    /// Primary label (e.g., series name)
    pub label: String,
    /// Primary value
    pub value: String,
    /// Optional secondary text
    pub secondary: Option<String>,
    /// Color indicator (CSS color value)
    pub color: Option<String>,
}

impl TooltipContent {
    /// Create a simple tooltip with label and value.
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            secondary: None,
            color: None,
        }
    }

    /// Add a color indicator.
    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Add secondary text.
    pub fn with_secondary(mut self, text: impl Into<String>) -> Self {
        self.secondary = Some(text.into());
        self
    }
}

/// Accessible glass-styled chart tooltip.
///
/// Uses `<foreignObject>` for HTML content inside SVG.
#[component]
pub fn ChartTooltip(
    /// Whether the tooltip is visible
    visible: Signal<bool>,
    /// X position in SVG coordinates
    x: Signal<f64>,
    /// Y position in SVG coordinates
    y: Signal<f64>,
    /// Tooltip content
    content: Signal<TooltipContent>,
    /// Chart dimensions for bounds checking
    #[prop(default = (400.0, 200.0))]
    bounds: (f64, f64),
    /// Anchor position
    #[prop(default = TooltipAnchor::Auto)]
    anchor: TooltipAnchor,
) -> impl IntoView {
    // Calculate tooltip position with bounds checking
    let tooltip_width = 120.0;
    let tooltip_height = 50.0;
    let offset = 10.0;

    let position = move || {
        let raw_x = x.get();
        let raw_y = y.get();
        let (width, height) = bounds;

        // Horizontal positioning - keep within bounds
        let final_x = (raw_x - tooltip_width / 2.0)
            .max(5.0)
            .min(width - tooltip_width - 5.0);

        // Vertical positioning based on anchor
        let final_y = match anchor {
            TooltipAnchor::Top => raw_y - tooltip_height - offset,
            TooltipAnchor::Bottom => raw_y + offset,
            TooltipAnchor::Auto => {
                if raw_y > height / 2.0 {
                    // Bottom half - show above
                    raw_y - tooltip_height - offset
                } else {
                    // Top half - show below
                    raw_y + offset
                }
            }
        };

        (final_x, final_y.max(5.0).min(height - tooltip_height - 5.0))
    };

    view! {
        <foreignObject
            x={move || position().0}
            y={move || position().1}
            width={tooltip_width}
            height={tooltip_height}
            class="chart-tooltip-container"
            style:opacity={move || if visible.get() { "1" } else { "0" }}
            style:visibility={move || if visible.get() { "visible" } else { "hidden" }}
            style:pointer-events="none"
            aria-hidden={move || (!visible.get()).to_string()}
        >
            <div
                class="chart-tooltip glass-panel"
                data-elevation="3"
                role="tooltip"
                aria-live="polite"
            >
                {move || {
                    let c = content.get();
                    view! {
                        <div class="chart-tooltip-inner">
                            // Color indicator
                            {c.color.clone().map(|color| view! {
                                <span
                                    class="chart-tooltip-color"
                                    style:background-color={color}
                                />
                            })}

                            <div class="chart-tooltip-text">
                                <span class="chart-tooltip-label">{c.label.clone()}</span>
                                <span class="chart-tooltip-value">{c.value.clone()}</span>
                                {c.secondary.clone().map(|s| view! {
                                    <span class="chart-tooltip-secondary">{s}</span>
                                })}
                            </div>
                        </div>
                    }
                }}
            </div>
        </foreignObject>
    }
}

/// Simple tooltip state manager for charts.
#[derive(Clone)]
pub struct TooltipState {
    pub visible: RwSignal<bool>,
    pub x: RwSignal<f64>,
    pub y: RwSignal<f64>,
    pub content: RwSignal<TooltipContent>,
}

impl Default for TooltipState {
    fn default() -> Self {
        Self::new()
    }
}

impl TooltipState {
    /// Create a new tooltip state.
    pub fn new() -> Self {
        Self {
            visible: RwSignal::new(false),
            x: RwSignal::new(0.0),
            y: RwSignal::new(0.0),
            content: RwSignal::new(TooltipContent::default()),
        }
    }

    /// Show the tooltip at a position with content.
    pub fn show(&self, x: f64, y: f64, content: TooltipContent) {
        self.x.set(x);
        self.y.set(y);
        self.content.set(content);
        self.visible.set(true);
    }

    /// Hide the tooltip.
    pub fn hide(&self) {
        self.visible.set(false);
    }

    /// Get read-only signals for the tooltip component.
    pub fn signals(
        &self,
    ) -> (
        Signal<bool>,
        Signal<f64>,
        Signal<f64>,
        Signal<TooltipContent>,
    ) {
        (
            self.visible.into(),
            self.x.into(),
            self.y.into(),
            self.content.into(),
        )
    }
}
