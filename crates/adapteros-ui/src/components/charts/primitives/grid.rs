//! SVG grid component for chart backgrounds.

use leptos::prelude::*;

use crate::components::charts::utils::{InvertedYScale, TimeScale};

/// Chart dimensions and padding configuration.
#[derive(Debug, Clone, Copy)]
pub struct ChartLayout {
    pub width: f64,
    pub height: f64,
    pub padding_left: f64,
    pub padding_right: f64,
    pub padding_top: f64,
    pub padding_bottom: f64,
}

impl ChartLayout {
    /// Create a new chart layout with equal padding.
    pub fn new(width: f64, height: f64, padding: f64) -> Self {
        Self {
            width,
            height,
            padding_left: padding,
            padding_right: padding,
            padding_top: padding,
            padding_bottom: padding,
        }
    }

    /// Create a layout with axis labels space.
    pub fn with_axes(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            padding_left: 50.0, // Y-axis labels
            padding_right: 20.0,
            padding_top: 20.0,
            padding_bottom: 30.0, // X-axis labels
        }
    }

    /// Get the chart area width (excluding padding).
    pub fn chart_width(&self) -> f64 {
        self.width - self.padding_left - self.padding_right
    }

    /// Get the chart area height (excluding padding).
    pub fn chart_height(&self) -> f64 {
        self.height - self.padding_top - self.padding_bottom
    }

    /// Get the left edge of the chart area.
    pub fn left(&self) -> f64 {
        self.padding_left
    }

    /// Get the right edge of the chart area.
    pub fn right(&self) -> f64 {
        self.width - self.padding_right
    }

    /// Get the top edge of the chart area.
    pub fn top(&self) -> f64 {
        self.padding_top
    }

    /// Get the bottom edge of the chart area.
    pub fn bottom(&self) -> f64 {
        self.height - self.padding_bottom
    }
}

impl Default for ChartLayout {
    fn default() -> Self {
        Self::with_axes(400.0, 200.0)
    }
}

/// Background grid component for charts.
#[component]
pub fn Grid(
    /// Chart layout configuration
    layout: ChartLayout,
    /// Number of horizontal grid lines
    #[prop(default = 5)]
    h_lines: usize,
    /// Number of vertical grid lines
    #[prop(default = 6)]
    v_lines: usize,
    /// Show horizontal lines
    #[prop(default = true)]
    show_horizontal: bool,
    /// Show vertical lines
    #[prop(default = true)]
    show_vertical: bool,
) -> impl IntoView {
    let left = layout.left();
    let right = layout.right();
    let top = layout.top();
    let bottom = layout.bottom();
    let chart_height = layout.chart_height();
    let chart_width = layout.chart_width();

    view! {
        <g class="chart-grid" aria-hidden="true">
            // Horizontal lines
            {move || {
                if show_horizontal && h_lines > 0 {
                    let step = chart_height / h_lines as f64;
                    (0..=h_lines)
                        .map(|i| {
                            let y = top + i as f64 * step;
                            view! {
                                <line
                                    x1={left}
                                    y1={y}
                                    x2={right}
                                    y2={y}
                                    class="chart-grid-line chart-grid-line-h"
                                />
                            }
                        })
                        .collect_view()
                        .into_any()
                } else {
                    ().into_any()
                }
            }}

            // Vertical lines
            {move || {
                if show_vertical && v_lines > 0 {
                    let step = chart_width / v_lines as f64;
                    (0..=v_lines)
                        .map(|i| {
                            let x = left + i as f64 * step;
                            view! {
                                <line
                                    x1={x}
                                    y1={top}
                                    x2={x}
                                    y2={bottom}
                                    class="chart-grid-line chart-grid-line-v"
                                />
                            }
                        })
                        .collect_view()
                        .into_any()
                } else {
                    ().into_any()
                }
            }}
        </g>
    }
}

/// X-axis component with tick marks and labels.
#[component]
pub fn XAxis(
    /// Chart layout configuration
    layout: ChartLayout,
    /// Time scale for mapping values to positions
    x_scale: TimeScale,
    /// Number of ticks
    #[prop(default = 6)]
    tick_count: usize,
    /// Label formatter function
    #[prop(optional)]
    format_label: Option<fn(u64) -> String>,
) -> impl IntoView {
    let y = layout.bottom();
    let ticks = x_scale.ticks(tick_count);
    let (time_min, time_max) = x_scale.time_range();
    let range_ms = time_max.saturating_sub(time_min);

    view! {
        <g class="chart-axis chart-axis-x" aria-hidden="true">
            // Axis line
            <line
                x1={layout.left()}
                y1={y}
                x2={layout.right()}
                y2={y}
                class="chart-axis-line"
            />

            // Ticks and labels
            {ticks
                .into_iter()
                .map(|tick_time| {
                    let x = x_scale.scale(tick_time);
                    let label = match format_label {
                        Some(f) => f(tick_time),
                        None => crate::components::charts::utils::format::format_time(tick_time, range_ms),
                    };
                    view! {
                        <g class="chart-tick">
                            <line
                                x1={x}
                                y1={y}
                                x2={x}
                                y2={y + 5.0}
                                class="chart-tick-line"
                            />
                            <text
                                x={x}
                                y={y + 18.0}
                                text-anchor="middle"
                                class="chart-tick-label"
                            >
                                {label}
                            </text>
                        </g>
                    }
                })
                .collect_view()}
        </g>
    }
}

/// Y-axis component with tick marks and labels.
#[component]
pub fn YAxis(
    /// Chart layout configuration
    layout: ChartLayout,
    /// Y scale for mapping values to positions
    y_scale: InvertedYScale,
    /// Number of ticks
    #[prop(default = 5)]
    tick_count: usize,
    /// Axis label (unit)
    #[prop(default = None)]
    label: Option<String>,
    /// Label formatter function
    #[prop(optional)]
    format_label: Option<fn(f64) -> String>,
) -> impl IntoView {
    let x = layout.left();
    let ticks = y_scale.nice_ticks(tick_count);

    let formatter = format_label.unwrap_or(crate::components::charts::utils::format::format_number);

    view! {
        <g class="chart-axis chart-axis-y" aria-hidden="true">
            // Axis line
            <line
                x1={x}
                y1={layout.top()}
                x2={x}
                y2={layout.bottom()}
                class="chart-axis-line"
            />

            // Ticks and labels
            {ticks
                .into_iter()
                .map(|tick_value| {
                    let y = y_scale.scale(tick_value);
                    let label = formatter(tick_value);
                    view! {
                        <g class="chart-tick">
                            <line
                                x1={x - 5.0}
                                y1={y}
                                x2={x}
                                y2={y}
                                class="chart-tick-line"
                            />
                            <text
                                x={x - 8.0}
                                y={y}
                                text-anchor="end"
                                dominant-baseline="middle"
                                class="chart-tick-label"
                            >
                                {label}
                            </text>
                        </g>
                    }
                })
                .collect_view()}

            // Axis label (rotated)
            {label.map(|l| {
                let mid_y = (layout.top() + layout.bottom()) / 2.0;
                view! {
                    <text
                        x={12.0}
                        y={mid_y}
                        text-anchor="middle"
                        dominant-baseline="middle"
                        transform={format!("rotate(-90, 12, {})", mid_y)}
                        class="chart-axis-label"
                    >
                        {l}
                    </text>
                }
            })}
        </g>
    }
}
