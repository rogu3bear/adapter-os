//! Line chart component for time series visualization.

use leptos::prelude::*;

use crate::components::charts::primitives::{
    build_line_path, ChartLayout, ChartTooltip, Grid, TooltipContent, TooltipState, XAxis, YAxis,
};
use crate::components::charts::types::{colors, TimeSeriesData};
use crate::components::charts::utils::{format_number, InvertedYScale, TimeScale};

/// Pre-computed chart data to avoid redundant calculations per render
#[derive(Clone, PartialEq)]
struct ChartComputedData {
    x_scale: TimeScale,
    y_scale: InvertedYScale,
    series_paths: Vec<SeriesPath>,
    series_count: usize,
    point_count: usize,
}

#[derive(Clone, PartialEq)]
struct SeriesPath {
    path: String,
    color: String,
    name: String,
}

impl ChartComputedData {
    fn compute(data: &TimeSeriesData, layout: &ChartLayout) -> Self {
        let (x_min, x_max) = data.calc_x_range();
        let (y_min, y_max) = data.calc_y_range();

        let x_scale = TimeScale::new((x_min, x_max), (layout.left(), layout.right()));
        let y_scale = InvertedYScale::new((y_min, y_max), (layout.top(), layout.bottom()));

        let series_paths = data
            .series
            .iter()
            .enumerate()
            .map(|(idx, series)| {
                let path = build_line_path(&series.points, &x_scale, &y_scale);
                let color = if series.color.is_empty() {
                    colors::palette(idx).to_string()
                } else {
                    series.color.clone()
                };
                SeriesPath {
                    path,
                    color,
                    name: series.name.clone(),
                }
            })
            .collect();

        Self {
            x_scale,
            y_scale,
            series_paths,
            series_count: data.series.len(),
            point_count: data.point_count(),
        }
    }
}

/// Line chart for time series data.
///
/// Supports multiple series, grid, axes, and interactive tooltips.
#[component]
pub fn LineChart(
    /// Time series data with one or more series
    #[prop(into)]
    data: Signal<TimeSeriesData>,
    /// Chart width in viewBox units
    #[prop(default = 400.0)]
    width: f64,
    /// Chart height in viewBox units
    #[prop(default = 200.0)]
    height: f64,
    /// Chart title
    #[prop(optional, into)]
    title: Option<String>,
    /// Y-axis label
    #[prop(optional, into)]
    y_label: Option<String>,
    /// Show background grid
    #[prop(default = true)]
    show_grid: bool,
    /// Show X axis
    #[prop(default = true)]
    show_x_axis: bool,
    /// Show Y axis
    #[prop(default = true)]
    show_y_axis: bool,
    /// Show interactive tooltips
    #[prop(default = true)]
    show_tooltip: bool,
    /// Show data point markers
    #[prop(default = false)]
    show_points: bool,
    /// Additional CSS class
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let layout = ChartLayout::with_axes(width, height);
    let tooltip = TooltipState::new();

    // Clone tooltip for different closures
    let tooltip_for_mouseleave = tooltip.clone();
    let tooltip_for_points = tooltip.clone();
    let tooltip_for_display = tooltip.clone();

    // Clone title for different uses
    let title_for_aria = title.clone();
    let title_for_display = title.clone();
    let has_title = title.is_some();

    // Memoize all computed chart data - single computation per data change
    let computed = Memo::new(move |_| data.with(|d| ChartComputedData::compute(d, &layout)));

    // Chart accessibility label - derived from memoized data
    let aria_label = move || {
        let c = computed.get();
        title_for_aria.clone().unwrap_or_else(|| {
            format!(
                "Line chart with {} series and {} data points",
                c.series_count, c.point_count
            )
        })
    };

    // Build SVG ID for accessibility
    let chart_id = format!("line-chart-{}", uuid::Uuid::new_v4());
    let title_id = format!("{}-title", chart_id);
    let title_id_for_svg = title_id.clone();

    view! {
        <div class={format!("chart-container glass-panel {}", class)} data-elevation="2">
            // Title (outside SVG for better styling)
            {title_for_display.map(|t| view! {
                <h3 class="chart-title" id={title_id.clone()}>{t}</h3>
            })}

            <svg
                class="chart-svg"
                viewBox={format!("0 0 {} {}", width, height)}
                preserveAspectRatio="xMidYMid meet"
                role="img"
                aria-labelledby={has_title.then(|| title_id_for_svg.clone())}
                aria-label={aria_label}
                on:mouseleave=move |_| tooltip_for_mouseleave.hide()
            >
                // Background grid
                {move || show_grid.then(|| view! {
                    <Grid layout={layout} />
                })}

                // X axis
                {move || show_x_axis.then(|| view! {
                    <XAxis layout={layout} x_scale={computed.get().x_scale} />
                })}

                // Y axis
                {move || show_y_axis.then(|| {
                    view! {
                        <YAxis
                            layout={layout}
                            y_scale={computed.get().y_scale}
                            label={y_label.clone()}
                        />
                    }
                })}

                // Chart area clipping
                <defs>
                    <clipPath id={format!("{}-clip", chart_id)}>
                        <rect
                            x={layout.left()}
                            y={layout.top()}
                            width={layout.chart_width()}
                            height={layout.chart_height()}
                        />
                    </clipPath>
                </defs>

                // Series lines - uses pre-computed paths from memo
                <g
                    class="chart-series"
                    clip-path={format!("url(#{}-clip)", chart_id)}
                >
                    {move || {
                        let c = computed.get();
                        c.series_paths.iter().map(|sp| {
                            view! {
                                <path
                                    d={sp.path.clone()}
                                    fill="none"
                                    stroke={sp.color.clone()}
                                    stroke-width="2"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                    class="chart-line"
                                    data-series={sp.name.clone()}
                                />
                            }
                        }).collect_view()
                    }}
                </g>

                // Data point markers (if enabled) - uses memoized scales
                {move || show_points.then(|| {
                    let c = computed.get();
                    let tooltip_ref = tooltip_for_points.clone();

                    // Access data only when points are shown
                    data.with(|d| view! {
                        <g class="chart-points">
                            {d.series.iter().enumerate().map(|(idx, series)| {
                                let color = if series.color.is_empty() {
                                    colors::palette(idx).to_string()
                                } else {
                                    series.color.clone()
                                };
                                let series_name = series.name.clone();
                                let tooltip_for_series = tooltip_ref.clone();

                                series.points.iter().map(|point| {
                                    let x = c.x_scale.scale(point.timestamp);
                                    let y = c.y_scale.scale(point.value);
                                    let value = point.value;
                                    let color_for_stroke = color.clone();
                                    let color_for_enter = color.clone();
                                    let color_for_focus = color.clone();
                                    let name_for_aria = series_name.clone();
                                    let name_for_enter = series_name.clone();
                                    let name_for_focus = series_name.clone();
                                    let tooltip_for_enter = tooltip_for_series.clone();
                                    let tooltip_for_focus = tooltip_for_series.clone();
                                    let tooltip_for_leave = tooltip_for_series.clone();
                                    let tooltip_for_blur = tooltip_for_series.clone();

                                    view! {
                                        <circle
                                            cx={x}
                                            cy={y}
                                            r="4"
                                            fill="var(--color-background)"
                                            stroke={color_for_stroke}
                                            stroke-width="2"
                                            class="chart-point"
                                            tabindex="0"
                                            role="graphics-symbol"
                                            aria-label={format!("{}: {:.2}", name_for_aria, value)}
                                            on:mouseenter=move |_| {
                                                tooltip_for_enter.show(x, y, TooltipContent::new(
                                                    name_for_enter.clone(),
                                                    format_number(value)
                                                ).with_color(color_for_enter.clone()));
                                            }
                                            on:focus=move |_| {
                                                tooltip_for_focus.show(x, y, TooltipContent::new(
                                                    name_for_focus.clone(),
                                                    format_number(value)
                                                ).with_color(color_for_focus.clone()));
                                            }
                                            on:mouseleave=move |_| tooltip_for_leave.hide()
                                            on:blur=move |_| tooltip_for_blur.hide()
                                        />
                                    }
                                }).collect_view()
                            }).collect_view()}
                        </g>
                    })
                })}

                // Tooltip
                {move || show_tooltip.then(|| {
                    let (visible, x, y, content) = tooltip_for_display.signals();
                    view! {
                        <ChartTooltip
                            visible={visible}
                            x={x}
                            y={y}
                            content={content}
                            bounds={(width, height)}
                        />
                    }
                })}
            </svg>

            // Legend (if multiple series) - uses pre-computed series info
            {move || {
                let c = computed.get();
                if c.series_count > 1 {
                    Some(view! {
                        <div class="chart-legend">
                            {c.series_paths.iter().map(|sp| {
                                view! {
                                    <div class="chart-legend-item">
                                        <span
                                            class="chart-legend-color"
                                            style:background-color={sp.color.clone()}
                                        />
                                        <span class="chart-legend-label">{sp.name.clone()}</span>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    })
                } else {
                    None
                }
            }}
        </div>
    }
}

/// Pre-computed data for MiniLineChart
#[derive(Clone, PartialEq)]
struct MiniChartData {
    line_path: String,
    area_path: Option<String>,
    has_data: bool,
}

impl MiniChartData {
    fn compute(data: &TimeSeriesData, width: f64, height: f64, padding: f64, fill: bool) -> Self {
        if !data.has_data() {
            return Self {
                line_path: String::new(),
                area_path: None,
                has_data: false,
            };
        }

        let (x_min, x_max) = data.calc_x_range();
        let (y_min, y_max) = data.calc_y_range();
        let x_scale = TimeScale::new((x_min, x_max), (padding, width - padding));
        let y_scale = InvertedYScale::new((y_min, y_max), (padding, height - padding));

        let series = &data.series[0];
        let line_path = build_line_path(&series.points, &x_scale, &y_scale);

        let area_path = if fill {
            if let (Some(first), Some(last)) = (series.points.first(), series.points.last()) {
                let first_x = x_scale.scale(first.timestamp);
                let last_x = x_scale.scale(last.timestamp);
                Some(format!(
                    "{} L {:.1},{:.1} L {:.1},{:.1} Z",
                    line_path,
                    last_x,
                    height - padding,
                    first_x,
                    height - padding
                ))
            } else {
                None
            }
        } else {
            None
        };

        Self {
            line_path,
            area_path,
            has_data: true,
        }
    }
}

/// Compact line chart without axes (for dashboards).
#[component]
pub fn MiniLineChart(
    /// Time series data
    #[prop(into)]
    data: Signal<TimeSeriesData>,
    /// Chart width
    #[prop(default = 200.0)]
    width: f64,
    /// Chart height
    #[prop(default = 60.0)]
    height: f64,
    /// Stroke color
    #[prop(optional, into)]
    color: Option<String>,
    /// Show area fill
    #[prop(default = true)]
    fill: bool,
    /// Additional CSS class
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let padding = 4.0;
    let stroke_color = color.unwrap_or_else(|| colors::PRIMARY.to_string());

    // Memoize all computed chart data
    let chart_data =
        Memo::new(move |_| data.with(|d| MiniChartData::compute(d, width, height, padding, fill)));

    view! {
        <svg
            class={format!("mini-line-chart {}", class)}
            viewBox={format!("0 0 {} {}", width, height)}
            preserveAspectRatio="xMidYMid meet"
            role="img"
            aria-label="Mini line chart"
        >
            {move || {
                let cd = chart_data.get();
                if !cd.has_data {
                    return view! {
                        <text
                            x={width / 2.0}
                            y={height / 2.0}
                            text-anchor="middle"
                            dominant-baseline="middle"
                            class="chart-no-data"
                        >
                            "No data"
                        </text>
                    }.into_any();
                }

                view! {
                    <g>
                        // Area fill
                        {cd.area_path.as_ref().map(|area_path| view! {
                            <path
                                d={area_path.clone()}
                                fill={stroke_color.clone()}
                                fill-opacity="0.1"
                                class="mini-chart-area"
                            />
                        })}

                        // Line
                        <path
                            d={cd.line_path.clone()}
                            fill="none"
                            stroke={stroke_color.clone()}
                            stroke-width="1.5"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            class="mini-chart-line"
                        />
                    </g>
                }.into_any()
            }}
        </svg>
    }
}
