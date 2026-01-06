//! Sparkline component - compact inline chart for metrics cards.

use leptos::prelude::*;

use crate::components::charts::primitives::path::{build_area_path, build_sparkline_path};
use crate::components::charts::types::colors;

/// Trend direction indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trend {
    Up,
    Down,
    Flat,
}

impl Trend {
    /// Determine trend from a series of values.
    pub fn from_values(values: &[f64]) -> Self {
        if values.len() < 2 {
            return Self::Flat;
        }

        // Compare first third average with last third average
        let third = values.len() / 3;
        if third == 0 {
            let first = values.first().copied().unwrap_or(0.0);
            let last = values.last().copied().unwrap_or(0.0);
            return if last > first * 1.05 {
                Self::Up
            } else if last < first * 0.95 {
                Self::Down
            } else {
                Self::Flat
            };
        }

        let first_avg: f64 = values[..third].iter().sum::<f64>() / third as f64;
        let last_avg: f64 = values[values.len() - third..].iter().sum::<f64>() / third as f64;

        if last_avg > first_avg * 1.05 {
            Self::Up
        } else if last_avg < first_avg * 0.95 {
            Self::Down
        } else {
            Self::Flat
        }
    }

    /// Get the color for this trend.
    pub fn color(&self) -> &'static str {
        match self {
            Self::Up => colors::SUCCESS,
            Self::Down => colors::ERROR,
            Self::Flat => colors::MUTED,
        }
    }
}

/// Compact sparkline chart for inline display.
///
/// Shows a simple line chart without axes or labels,
/// suitable for embedding in metric cards or tables.
#[component]
pub fn Sparkline(
    /// Data values (Y-axis only, X is implicit index)
    #[prop(into)]
    values: Signal<Vec<f64>>,
    /// Width in pixels
    #[prop(default = 80)]
    width: u32,
    /// Height in pixels
    #[prop(default = 24)]
    height: u32,
    /// Stroke color (CSS value, defaults to primary)
    #[prop(optional, into)]
    color: Option<String>,
    /// Show area fill under the line
    #[prop(default = false)]
    fill: bool,
    /// Color based on trend direction
    #[prop(default = false)]
    trend_color: bool,
    /// Accessible label
    #[prop(optional, into)]
    label: Option<String>,
    /// Additional CSS class
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let padding = 2.0;
    let w = width as f64;
    let h = height as f64;

    // Clone color for use in closures
    let base_color = color.clone().unwrap_or_else(|| colors::PRIMARY.to_string());
    let base_color_for_area = base_color.clone();
    let base_color_for_line = base_color.clone();
    let base_color_for_dot = base_color.clone();

    // Build paths reactively
    let line_path = move || {
        let vals = values.get();
        build_sparkline_path(&vals, w, h, padding)
    };

    let area_path = move || {
        if !fill {
            return String::new();
        }
        let vals = values.get();
        build_area_path(&vals, w, h, padding)
    };

    // Clone label for use in closure
    let label_for_aria = label.clone();

    // Compute accessible label
    let aria_label = move || {
        let vals = values.get();
        label_for_aria.clone().unwrap_or_else(|| {
            if vals.is_empty() {
                "No data".to_string()
            } else {
                let min = vals.iter().cloned().fold(f64::MAX, f64::min);
                let max = vals.iter().cloned().fold(f64::MIN, f64::max);
                let last = vals.last().copied().unwrap_or(0.0);
                format!(
                    "Sparkline: {} points, range {:.1} to {:.1}, current {:.1}",
                    vals.len(),
                    min,
                    max,
                    last
                )
            }
        })
    };

    view! {
        <svg
            width={width}
            height={height}
            viewBox={format!("0 0 {} {}", width, height)}
            preserveAspectRatio="xMinYMid meet"
            role="img"
            aria-label={aria_label}
            class={format!("sparkline {}", class)}
        >
            // Area fill (if enabled)
            {move || {
                if fill {
                    let path = area_path();
                    let vals = values.get();
                    let color = if trend_color {
                        Trend::from_values(&vals).color().to_string()
                    } else {
                        base_color_for_area.clone()
                    };
                    if !path.is_empty() {
                        Some(view! {
                            <path
                                d={path}
                                fill={color}
                                fill-opacity="0.1"
                                class="sparkline-area"
                            />
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }}

            // Line
            {move || {
                let path = line_path();
                let vals = values.get();
                let color = if trend_color {
                    Trend::from_values(&vals).color().to_string()
                } else {
                    base_color_for_line.clone()
                };
                if !path.is_empty() {
                    Some(view! {
                        <path
                            d={path}
                            fill="none"
                            stroke={color}
                            stroke-width="1.5"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            class="sparkline-line"
                        />
                    })
                } else {
                    None
                }
            }}

            // Current value indicator (dot at end)
            {move || {
                let vals = values.get();
                if vals.is_empty() {
                    return None;
                }

                let x = padding + (w - padding * 2.0);
                let y_min = vals.iter().cloned().fold(f64::MAX, f64::min);
                let y_max = vals.iter().cloned().fold(f64::MIN, f64::max);
                let y_range = if (y_max - y_min).abs() < f64::EPSILON {
                    1.0
                } else {
                    y_max - y_min
                };

                let last_val = vals.last().copied().unwrap_or(0.0);
                let normalized = (last_val - y_min) / y_range;
                let y = padding + (1.0 - normalized) * (h - padding * 2.0);

                let color = if trend_color {
                    Trend::from_values(&vals).color().to_string()
                } else {
                    base_color_for_dot.clone()
                };

                Some(view! {
                    <circle
                        cx={x}
                        cy={y}
                        r="2"
                        fill={color}
                        class="sparkline-dot"
                    />
                })
            }}
        </svg>
    }
}

/// Sparkline with label and value display.
///
/// Shows a sparkline alongside the current value and optional trend indicator.
#[component]
pub fn SparklineMetric(
    /// Metric label
    #[prop(into)]
    label: String,
    /// Current value (formatted)
    #[prop(into)]
    value: String,
    /// Historical values for sparkline
    #[prop(into)]
    values: Signal<Vec<f64>>,
    /// Unit suffix (e.g., "ms", "%")
    #[prop(optional, into)]
    unit: Option<String>,
    /// Show trend indicator
    #[prop(default = true)]
    show_trend: bool,
    /// Sparkline width
    #[prop(default = 60)]
    sparkline_width: u32,
    /// Additional CSS class
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let trend = move || Trend::from_values(&values.get());

    let trend_icon = move || match trend() {
        Trend::Up => "↑",
        Trend::Down => "↓",
        Trend::Flat => "→",
    };

    view! {
        <div class={format!("sparkline-metric {}", class)}>
            <div class="sparkline-metric-header">
                <span class="sparkline-metric-label">{label}</span>
                {show_trend.then(|| view! {
                    <span
                        class="sparkline-metric-trend"
                        style:color={move || trend().color()}
                    >
                        {trend_icon}
                    </span>
                })}
            </div>
            <div class="sparkline-metric-body">
                <span class="sparkline-metric-value">
                    {value}
                    {unit.map(|u| view! { <span class="sparkline-metric-unit">{u}</span> })}
                </span>
                <Sparkline
                    values={values}
                    width={sparkline_width}
                    height=20
                    trend_color=true
                    fill=true
                />
            </div>
        </div>
    }
}
