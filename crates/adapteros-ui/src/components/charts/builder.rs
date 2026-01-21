//! Declarative chart helpers for composable chart creation.
//!
//! Provides helper functions for common chart configurations:
//!
//! ```ignore
//! use adapteros_ui::components::charts::chart_helpers;
//!
//! // Quick line chart
//! let chart = chart_helpers::line_chart("Throughput", "req/s", data_signal);
//!
//! // Quick mini chart  
//! let chart = chart_helpers::mini_chart(data_signal, Some("red".to_string()));
//! ```

use leptos::prelude::*;

use super::line_chart::{LineChart, MiniLineChart};
use super::types::TimeSeriesData;

/// Create a line chart with title and y-axis label
pub fn line_chart(
    title: impl Into<String>,
    y_label: impl Into<String>,
    data: Signal<TimeSeriesData>,
) -> impl IntoView {
    view! {
        <LineChart
            data=data
            title=title.into()
            y_label=y_label.into()
        />
    }
}

/// Create a line chart with custom dimensions
pub fn line_chart_sized(
    title: impl Into<String>,
    y_label: impl Into<String>,
    width: f64,
    height: f64,
    data: Signal<TimeSeriesData>,
) -> impl IntoView {
    view! {
        <LineChart
            data=data
            title=title.into()
            y_label=y_label.into()
            width=width
            height=height
        />
    }
}

/// Create a minimal line chart without title
pub fn line_chart_minimal(data: Signal<TimeSeriesData>) -> impl IntoView {
    view! {
        <LineChart
            data=data
            show_grid=false
            show_x_axis=true
            show_y_axis=true
        />
    }
}

/// Create a compact mini chart
pub fn mini_chart(data: Signal<TimeSeriesData>, color: Option<String>) -> impl IntoView {
    match color {
        Some(c) => view! {
            <MiniLineChart
                data=data
                color=c
                fill=true
            />
        }
        .into_any(),
        None => view! {
            <MiniLineChart
                data=data
                fill=true
            />
        }
        .into_any(),
    }
}

/// Create a mini chart with custom dimensions
pub fn mini_chart_sized(
    width: f64,
    height: f64,
    data: Signal<TimeSeriesData>,
    color: Option<String>,
) -> impl IntoView {
    match color {
        Some(c) => view! {
            <MiniLineChart
                data=data
                width=width
                height=height
                color=c
                fill=true
            />
        }
        .into_any(),
        None => view! {
            <MiniLineChart
                data=data
                width=width
                height=height
                fill=true
            />
        }
        .into_any(),
    }
}
