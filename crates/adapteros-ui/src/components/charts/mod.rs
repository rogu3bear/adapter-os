//! Liquid Glass Charts - Pure Rust SVG visualization components.
//!
//! This module provides a minimal chart kit for the adapterOS dashboard:
//!
//! - **LineChart**: Time series visualization with tooltips and legends
//! - **Sparkline**: Compact inline charts for metric cards
//! - **StatusHeatmap**: Worker health grid visualization
//!
//! All components use pure SVG rendering via Leptos, with no JavaScript dependencies.
//! Charts integrate with the glass morphism design system for consistent styling.
//!
//! # Example
//!
//! ```rust,ignore
//! use adapteros_ui::components::charts::{LineChart, TimeSeriesData, DataSeries, ChartPoint};
//!
//! let data = TimeSeriesData::single(DataSeries {
//!     name: "Throughput".to_string(),
//!     color: "var(--color-primary)".to_string(),
//!     points: vec![
//!         ChartPoint::new(1000, 42.0),
//!         ChartPoint::new(2000, 45.0),
//!         ChartPoint::new(3000, 48.0),
//!     ],
//! });
//!
//! view! {
//!     <LineChart
//!         data={Signal::derive(move || data.clone())}
//!         title="Requests/sec"
//!         y_label="req/s"
//!     />
//! }
//! ```

pub mod heatmap;
pub mod line_chart;
pub mod primitives;
pub mod sparkline;
pub mod types;
pub mod utils;

// Re-export main types
pub use types::{
    colors, ChartPoint, DataSeries, HeatmapCell, HeatmapData, HeatmapRow, TimeSeriesData,
    WorkerStatus,
};

// Re-export components
pub use heatmap::{MiniHeatmap, StatusHeatmap};
pub use line_chart::{LineChart, MiniLineChart};
pub use sparkline::{Sparkline, SparklineMetric, Trend};

// Re-export primitives for custom charts
pub use primitives::{ChartLayout, ChartTooltip, Grid, TooltipContent, TooltipState, XAxis, YAxis};

// Re-export utilities
pub use utils::{
    escape_svg, format_duration, format_latency, format_number, format_percent, format_throughput,
    format_time, format_timestamp_full, InvertedYScale, LinearScale, TimeScale,
};
