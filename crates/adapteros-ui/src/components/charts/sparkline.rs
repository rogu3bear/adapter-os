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

/// Pre-computed sparkline data to avoid repeated calculations
#[derive(Clone, PartialEq)]
struct SparklineData {
    line_path: String,
    area_path: String,
    trend: Trend,
    dot_y: Option<f64>,
    aria_label: String,
    y_min: f64,
    y_max: f64,
}

impl SparklineData {
    fn compute(values: &[f64], w: f64, h: f64, padding: f64, custom_label: Option<&str>) -> Self {
        if values.is_empty() {
            return Self {
                line_path: String::new(),
                area_path: String::new(),
                trend: Trend::Flat,
                dot_y: None,
                aria_label: custom_label
                    .map(String::from)
                    .unwrap_or_else(|| "No data".to_string()),
                y_min: 0.0,
                y_max: 0.0,
            };
        }

        let y_min = values.iter().cloned().fold(f64::MAX, f64::min);
        let y_max = values.iter().cloned().fold(f64::MIN, f64::max);
        let y_range = if (y_max - y_min).abs() < f64::EPSILON {
            1.0
        } else {
            y_max - y_min
        };

        let last_val = values.last().copied().unwrap_or(0.0);
        let normalized = (last_val - y_min) / y_range;
        let dot_y = padding + (1.0 - normalized) * (h - padding * 2.0);

        let aria_label = custom_label.map(String::from).unwrap_or_else(|| {
            format!(
                "Sparkline: {} points, range {:.1} to {:.1}, current {:.1}",
                values.len(),
                y_min,
                y_max,
                last_val
            )
        });

        Self {
            line_path: build_sparkline_path(values, w, h, padding),
            area_path: build_area_path(values, w, h, padding),
            trend: Trend::from_values(values),
            dot_y: Some(dot_y),
            aria_label,
            y_min,
            y_max,
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

    let base_color = color.unwrap_or_else(|| colors::PRIMARY.to_string());

    // Memoize all sparkline data - single computation per values change
    let sparkline_data = Memo::new(move |_| {
        values.with(|vals| SparklineData::compute(vals, w, h, padding, label.as_deref()))
    });

    // Clone base_color for each closure that needs it
    let base_color_area = base_color.clone();
    let base_color_line = base_color.clone();
    let base_color_dot = base_color;

    let dot_x = padding + (w - padding * 2.0);

    view! {
        <svg
            width={width}
            height={height}
            viewBox={format!("0 0 {} {}", width, height)}
            preserveAspectRatio="xMinYMid meet"
            role="img"
            aria-label={move || sparkline_data.get().aria_label}
            class={format!("sparkline {}", class)}
        >
            // Area fill (if enabled)
            {move || {
                if !fill {
                    return None;
                }
                let data = sparkline_data.get();
                if data.area_path.is_empty() {
                    return None;
                }
                let color = if trend_color {
                    data.trend.color().to_string()
                } else {
                    base_color_area.clone()
                };
                Some(view! {
                    <path
                        d={data.area_path}
                        fill={color}
                        fill-opacity="0.1"
                        class="sparkline-area"
                    />
                })
            }}

            // Line
            {move || {
                let data = sparkline_data.get();
                if data.line_path.is_empty() {
                    return None;
                }
                let color = if trend_color {
                    data.trend.color().to_string()
                } else {
                    base_color_line.clone()
                };
                Some(view! {
                    <path
                        d={data.line_path}
                        fill="none"
                        stroke={color}
                        stroke-width="1.5"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        class="sparkline-line"
                    />
                })
            }}

            // Current value indicator (dot at end)
            {move || {
                let data = sparkline_data.get();
                let y = data.dot_y?;
                let color = if trend_color {
                    data.trend.color().to_string()
                } else {
                    base_color_dot.clone()
                };
                Some(view! {
                    <circle
                        cx={dot_x}
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
    // Memoize trend calculation - only recomputes when values change
    let trend = Memo::new(move |_| values.with(|vals| Trend::from_values(vals)));

    view! {
        <div class={format!("sparkline-metric {}", class)}>
            <div class="sparkline-metric-header">
                <span class="sparkline-metric-label">{label}</span>
                {show_trend.then(|| view! {
                    <span
                        class="sparkline-metric-trend"
                        style:color={move || trend.get().color()}
                    >
                        {move || match trend.get() {
                            Trend::Up => "↑",
                            Trend::Down => "↓",
                            Trend::Flat => "→",
                        }}
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
