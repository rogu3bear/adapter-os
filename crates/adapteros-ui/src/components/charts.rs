//! Lightweight chart components for dashboards.

use leptos::prelude::*;

/// Single data point for time series charts.
#[derive(Debug, Clone)]
pub struct ChartPoint {
    pub timestamp: u64,
    pub value: f64,
}

impl ChartPoint {
    pub fn new(timestamp: u64, value: f64) -> Self {
        Self { timestamp, value }
    }
}

/// Series data for charts.
#[derive(Debug, Clone)]
pub struct DataSeries {
    pub name: String,
    pub points: Vec<ChartPoint>,
    pub color: String,
}

/// Time series data wrapper.
#[derive(Debug, Clone, Default)]
pub struct TimeSeriesData {
    pub series: Vec<DataSeries>,
}

impl TimeSeriesData {
    pub fn new() -> Self {
        Self { series: Vec::new() }
    }
}

/// Heatmap data structure.
#[derive(Debug, Clone)]
pub struct HeatmapData {
    pub values: Vec<Vec<f64>>,
}

/// Worker status for status heatmap cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerStatus {
    Healthy,
    Warning,
    Error,
    Unknown,
}

fn series_path(points: &[ChartPoint]) -> String {
    if points.is_empty() {
        return String::new();
    }

    let values: Vec<f64> = points.iter().map(|p| p.value).collect();
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        max - min
    };

    let count = values.len();
    let mut path = String::new();

    for (idx, value) in values.into_iter().enumerate() {
        let x = if count <= 1 {
            0.0
        } else {
            (idx as f64) / ((count - 1) as f64) * 100.0
        };
        let normalized = (value - min) / range;
        let y = 100.0 - (normalized * 100.0);

        if idx == 0 {
            path.push_str(&format!("M {:.2} {:.2}", x, y));
        } else {
            path.push_str(&format!(" L {:.2} {:.2}", x, y));
        }
    }

    path
}

fn sparkline_path(values: &[f64]) -> String {
    let points: Vec<ChartPoint> = values
        .iter()
        .enumerate()
        .map(|(idx, value)| ChartPoint::new(idx as u64, *value))
        .collect();
    series_path(&points)
}

/// Sparkline component.
#[component]
pub fn Sparkline(values: Signal<Vec<f64>>, #[prop(optional)] height: Option<f64>) -> impl IntoView {
    let height = height.unwrap_or(32.0);

    view! {
        <svg
            class="sparkline"
            viewBox="0 0 100 100"
            preserveAspectRatio="none"
            style=format!("height: {}px", height)
        >
            <path
                d=move || sparkline_path(&values.get())
                class="sparkline-path"
            />
        </svg>
    }
}

/// Metric card with sparkline preview.
#[component]
pub fn SparklineMetric(
    #[prop(into)] label: String,
    #[prop(into)] value: String,
    #[prop(optional, into)] unit: Option<String>,
    values: Signal<Vec<f64>>,
    #[prop(optional)] show_trend: bool,
) -> impl IntoView {
    let trend = Signal::derive(move || {
        let vals = values.get();
        if vals.len() < 2 {
            return None;
        }
        let first = *vals.first().unwrap_or(&0.0);
        let last = *vals.last().unwrap_or(&0.0);
        let delta = last - first;
        Some(delta)
    });

    view! {
        <div class="sparkline-metric">
            <div class="sparkline-metric-header">
                <div>
                    <p class="sparkline-metric-label">{label}</p>
                    <div class="sparkline-metric-value">
                        <span>{value}</span>
                        {unit.map(|u| view! { <span class="sparkline-metric-unit">{u}</span> })}
                    </div>
                </div>
                <Sparkline values=values height=30.0/>
            </div>
            {move || {
                if show_trend {
                    trend.get().map(|delta| {
                        let class = if delta >= 0.0 {
                            "sparkline-metric-trend up"
                        } else {
                            "sparkline-metric-trend down"
                        };
                        let arrow = if delta >= 0.0 { "▲" } else { "▼" };
                        view! {
                            <div class=class>
                                <span>{arrow}</span>
                                <span>{format!("{:+.2}", delta)}</span>
                            </div>
                        }
                    })
                } else {
                    None
                }
            }}
        </div>
    }
}

/// Line chart component for time series data.
#[component]
pub fn LineChart(
    data: Signal<TimeSeriesData>,
    #[prop(into)] title: String,
    #[prop(into)] y_label: String,
    #[prop(optional)] height: f64,
    #[prop(optional)] show_points: bool,
) -> impl IntoView {
    let height = if height <= 0.0 { 200.0 } else { height };

    view! {
        <div class="chart">
            <div class="chart-header">
                <div>
                    <h3 class="chart-title">{title}</h3>
                    <p class="chart-subtitle">{y_label}</p>
                </div>
            </div>
            <svg
                class="chart-canvas"
                viewBox="0 0 100 100"
                preserveAspectRatio="none"
                style=format!("height: {}px", height)
            >
                {move || {
                    data.get().series.into_iter().map(|series| {
                        let points = series.points.clone();
                        let count = points.len();
                        let min = points.iter().map(|p| p.value).fold(f64::INFINITY, f64::min);
                        let max = points
                            .iter()
                            .map(|p| p.value)
                            .fold(f64::NEG_INFINITY, f64::max);
                        let range = if (max - min).abs() < f64::EPSILON { 1.0 } else { max - min };

                        let color = if series.color.is_empty() {
                            "var(--color-primary)".to_string()
                        } else {
                            series.color.clone()
                        };

                        view! {
                            <path
                                d=series_path(&points)
                                stroke=color.clone()
                                fill="none"
                                stroke-width="2"
                                class="chart-line"
                            />
                            {if show_points {
                                Some(view! {
                                    {points.into_iter().enumerate().map(|(idx, point)| {
                                        let x = if count <= 1 {
                                            0.0
                                        } else {
                                            (idx as f64) / ((count - 1) as f64) * 100.0
                                        };
                                        let y = 100.0 - ((point.value - min) / range * 100.0);

                                        view! {
                                            <circle cx=x cy=y r="1.5" fill=color.clone() class="chart-point" />
                                        }
                                    }).collect::<Vec<_>>()}
                                })
                            } else {
                                None
                            }}
                        }
                    }).collect::<Vec<_>>()
                }}
            </svg>
        </div>
    }
}

/// Compact line chart for inline usage.
#[component]
pub fn MiniLineChart(values: Signal<Vec<f64>>) -> impl IntoView {
    view! {
        <Sparkline values=values height=24.0/>
    }
}

/// Mini heatmap block.
#[component]
pub fn MiniHeatmap(data: HeatmapData) -> impl IntoView {
    view! {
        <div class="heatmap">
            {data.values.into_iter().map(|row| {
                view! {
                    <div class="heatmap-row">
                        {row.into_iter().map(|value| {
                            let class = if value > 0.7 {
                                "heatmap-cell high"
                            } else if value > 0.3 {
                                "heatmap-cell mid"
                            } else {
                                "heatmap-cell low"
                            };
                            view! { <span class=class></span> }
                        }).collect::<Vec<_>>()}
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}

/// Status heatmap using discrete worker status values.
#[component]
pub fn StatusHeatmap(statuses: Vec<WorkerStatus>) -> impl IntoView {
    view! {
        <div class="heatmap">
            <div class="heatmap-row">
                {statuses.into_iter().map(|status| {
                    let class = match status {
                        WorkerStatus::Healthy => "heatmap-cell ok",
                        WorkerStatus::Warning => "heatmap-cell warn",
                        WorkerStatus::Error => "heatmap-cell err",
                        WorkerStatus::Unknown => "heatmap-cell unk",
                    };
                    view! { <span class=class></span> }
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}
