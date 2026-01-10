//! Chart data types for Liquid Glass visualization components.
//!
//! These types define the data contracts for all chart components,
//! ensuring type-safe data binding between API responses and visualizations.

use serde::{Deserialize, Serialize};

/// A single data point in a time series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartPoint {
    /// Unix timestamp in milliseconds
    pub timestamp: u64,
    /// The data value
    pub value: f64,
    /// Optional label for tooltips
    pub label: Option<String>,
}

impl ChartPoint {
    /// Create a new chart point with timestamp and value.
    pub fn new(timestamp: u64, value: f64) -> Self {
        Self {
            timestamp,
            value,
            label: None,
        }
    }

    /// Create a new chart point with a label.
    pub fn with_label(timestamp: u64, value: f64, label: impl Into<String>) -> Self {
        Self {
            timestamp,
            value,
            label: Some(label.into()),
        }
    }
}

/// A named series of data points with styling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataSeries {
    /// Series name (used in legends and tooltips)
    pub name: String,
    /// CSS color value for the series line
    pub color: String,
    /// Data points in chronological order
    pub points: Vec<ChartPoint>,
}

impl DataSeries {
    /// Create a new empty data series.
    pub fn new(name: impl Into<String>, color: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            color: color.into(),
            points: Vec::new(),
        }
    }

    /// Add a point to the series.
    pub fn push(&mut self, point: ChartPoint) {
        self.points.push(point);
    }

    /// Keep only the last N points (for rolling windows).
    pub fn trim_to(&mut self, max_points: usize) {
        if self.points.len() > max_points {
            let drain_count = self.points.len() - max_points;
            self.points.drain(0..drain_count);
        }
    }
}

/// Time series data for line charts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeSeriesData {
    /// One or more data series
    pub series: Vec<DataSeries>,
    /// Optional fixed X-axis minimum (auto-calculated if None)
    pub x_min: Option<u64>,
    /// Optional fixed X-axis maximum (auto-calculated if None)
    pub x_max: Option<u64>,
    /// Optional fixed Y-axis minimum (auto-calculated if None)
    pub y_min: Option<f64>,
    /// Optional fixed Y-axis maximum (auto-calculated if None)
    pub y_max: Option<f64>,
}

impl Default for TimeSeriesData {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeSeriesData {
    /// Create empty time series data.
    pub fn new() -> Self {
        Self {
            series: Vec::new(),
            x_min: None,
            x_max: None,
            y_min: None,
            y_max: None,
        }
    }

    /// Create time series with a single series.
    pub fn single(series: DataSeries) -> Self {
        Self {
            series: vec![series],
            x_min: None,
            x_max: None,
            y_min: None,
            y_max: None,
        }
    }

    /// Add a series to the data.
    pub fn add_series(&mut self, series: DataSeries) {
        self.series.push(series);
    }

    /// Calculate the actual X range from data.
    pub fn calc_x_range(&self) -> (u64, u64) {
        let mut data_min: Option<u64> = None;
        let mut data_max: Option<u64> = None;

        for series in &self.series {
            for point in &series.points {
                data_min = Some(match data_min {
                    Some(min) => min.min(point.timestamp),
                    None => point.timestamp,
                });
                data_max = Some(match data_max {
                    Some(max) => max.max(point.timestamp),
                    None => point.timestamp,
                });
            }
        }

        let x_min = self.x_min.or(data_min).unwrap_or(0);
        let mut x_max = self.x_max.or(data_max).unwrap_or(x_min);

        // Ensure non-zero range
        if x_max <= x_min {
            if let Some(next) = x_min.checked_add(1) {
                x_max = next;
            } else {
                let prev = x_min.saturating_sub(1);
                return (prev, x_min);
            }
        }

        (x_min, x_max)
    }

    /// Calculate the actual Y range from data.
    pub fn calc_y_range(&self) -> (f64, f64) {
        let mut data_min: Option<f64> = None;
        let mut data_max: Option<f64> = None;

        for series in &self.series {
            for point in &series.points {
                if point.value.is_finite() {
                    data_min = Some(match data_min {
                        Some(min) => min.min(point.value),
                        None => point.value,
                    });
                    data_max = Some(match data_max {
                        Some(max) => max.max(point.value),
                        None => point.value,
                    });
                }
            }
        }

        let y_min = self.y_min.or(data_min).unwrap_or(0.0);
        let mut y_max = self.y_max.or(data_max).unwrap_or(y_min);

        // Ensure non-zero range with 10% headroom
        if y_max <= y_min || (y_max - y_min).abs() < f64::EPSILON {
            y_max = y_min + 1.0;
        } else {
            y_max *= 1.1;
        }

        (y_min, y_max)
    }

    /// Check if there's any data to display.
    pub fn has_data(&self) -> bool {
        self.series.iter().any(|s| !s.points.is_empty())
    }

    /// Get total point count across all series.
    pub fn point_count(&self) -> usize {
        self.series.iter().map(|s| s.points.len()).sum()
    }
}

/// Worker health status for heatmap cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerStatus {
    /// Worker is healthy and processing requests
    Healthy,
    /// Worker is experiencing degraded performance
    Degraded,
    /// Worker is draining (not accepting new requests)
    Draining,
    /// Worker is down or errored
    Down,
    /// Worker status is unknown
    Unknown,
}

impl WorkerStatus {
    /// Get the CSS color variable for this status.
    pub fn color(&self) -> &'static str {
        match self {
            Self::Healthy => "var(--color-green-500, #22c55e)",
            Self::Degraded => "var(--color-yellow-500, #eab308)",
            Self::Draining => "var(--color-orange-500, #f97316)",
            Self::Down => "var(--color-red-500, #ef4444)",
            Self::Unknown => "var(--color-muted, #6b7280)",
        }
    }

    /// Parse from status string (matches WorkerResponse.status).
    pub fn from_status_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "healthy" | "running" | "ready" => Self::Healthy,
            "degraded" | "warning" => Self::Degraded,
            "draining" => Self::Draining,
            "down" | "error" | "stopped" | "failed" => Self::Down,
            _ => Self::Unknown,
        }
    }
}

/// A single cell in a heatmap grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeatmapCell {
    /// Normalized value (0.0 = bad, 1.0 = good)
    pub value: f64,
    /// Status category for color mapping
    pub status: WorkerStatus,
    /// Tooltip content
    pub tooltip: Option<String>,
}

impl HeatmapCell {
    /// Create a cell from a health percentage.
    pub fn from_health(health_percent: f64) -> Self {
        let status = if health_percent >= 0.95 {
            WorkerStatus::Healthy
        } else if health_percent >= 0.7 {
            WorkerStatus::Degraded
        } else if health_percent >= 0.3 {
            WorkerStatus::Draining
        } else {
            WorkerStatus::Down
        };

        Self {
            value: health_percent,
            status,
            tooltip: Some(format!("{:.0}%", health_percent * 100.0)),
        }
    }

    /// Create a cell from worker status string.
    pub fn from_status(status_str: &str) -> Self {
        let status = WorkerStatus::from_status_str(status_str);
        let value = match status {
            WorkerStatus::Healthy => 1.0,
            WorkerStatus::Degraded => 0.7,
            WorkerStatus::Draining => 0.4,
            WorkerStatus::Down => 0.0,
            WorkerStatus::Unknown => 0.5,
        };

        Self {
            value,
            status,
            tooltip: Some(status_str.to_string()),
        }
    }
}

/// A row in the heatmap (one worker over time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeatmapRow {
    /// Row label (e.g., worker ID)
    pub label: String,
    /// Cells for each time bucket
    pub cells: Vec<HeatmapCell>,
}

impl HeatmapRow {
    /// Create a new row with a label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            cells: Vec::new(),
        }
    }

    /// Add a cell to the row.
    pub fn push(&mut self, cell: HeatmapCell) {
        self.cells.push(cell);
    }
}

/// Heatmap data for worker status visualization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeatmapData {
    /// Rows (one per worker)
    pub rows: Vec<HeatmapRow>,
    /// Column timestamps (time buckets)
    pub time_buckets: Vec<u64>,
    /// Column labels (formatted times)
    pub column_labels: Vec<String>,
}

impl Default for HeatmapData {
    fn default() -> Self {
        Self::new()
    }
}

impl HeatmapData {
    /// Create empty heatmap data.
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            time_buckets: Vec::new(),
            column_labels: Vec::new(),
        }
    }

    /// Check if there's any data.
    pub fn has_data(&self) -> bool {
        !self.rows.is_empty() && self.rows.iter().any(|r| !r.cells.is_empty())
    }

    /// Get grid dimensions (rows, cols).
    pub fn dimensions(&self) -> (usize, usize) {
        let rows = self.rows.len();
        let cols = self.rows.first().map(|r| r.cells.len()).unwrap_or(0);
        (rows, cols)
    }
}

/// Chart color palette using CSS variables.
pub mod colors {
    /// Primary chart line color
    pub const PRIMARY: &str = "var(--color-primary, #3b82f6)";
    /// Secondary chart line color
    pub const SECONDARY: &str = "var(--color-secondary, #8b5cf6)";
    /// Success/healthy color
    pub const SUCCESS: &str = "var(--color-green-500, #22c55e)";
    /// Warning color
    pub const WARNING: &str = "var(--color-yellow-500, #eab308)";
    /// Error/danger color
    pub const ERROR: &str = "var(--color-red-500, #ef4444)";
    /// Muted/neutral color
    pub const MUTED: &str = "var(--color-muted-foreground, #6b7280)";
    /// Grid line color
    pub const GRID: &str = "var(--color-border, #e5e7eb)";
    /// Axis text color
    pub const AXIS_TEXT: &str = "var(--color-muted-foreground, #6b7280)";

    /// Get a color from a palette by index (cycles).
    pub fn palette(index: usize) -> &'static str {
        const PALETTE: &[&str] = &[
            "var(--color-primary, #3b82f6)",
            "var(--color-secondary, #8b5cf6)",
            "var(--color-green-500, #22c55e)",
            "var(--color-yellow-500, #eab308)",
            "var(--color-orange-500, #f97316)",
            "var(--color-pink-500, #ec4899)",
            "var(--color-cyan-500, #06b6d4)",
        ];
        PALETTE[index % PALETTE.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chart_point_creation() {
        let point = ChartPoint::new(1000, 42.5);
        assert_eq!(point.timestamp, 1000);
        assert_eq!(point.value, 42.5);
        assert!(point.label.is_none());

        let labeled = ChartPoint::with_label(2000, 10.0, "test");
        assert_eq!(labeled.label, Some("test".to_string()));
    }

    #[test]
    fn test_data_series_trim() {
        let mut series = DataSeries::new("test", "#000");
        for i in 0..100 {
            series.push(ChartPoint::new(i, i as f64));
        }
        assert_eq!(series.points.len(), 100);

        series.trim_to(60);
        assert_eq!(series.points.len(), 60);
        assert_eq!(series.points[0].timestamp, 40); // Oldest removed
    }

    #[test]
    fn test_time_series_range_calculation() {
        let mut data = TimeSeriesData::new();
        let mut series = DataSeries::new("test", "#000");
        series.push(ChartPoint::new(100, 10.0));
        series.push(ChartPoint::new(200, 50.0));
        series.push(ChartPoint::new(300, 30.0));
        data.add_series(series);

        let (x_min, x_max) = data.calc_x_range();
        assert_eq!(x_min, 100);
        assert_eq!(x_max, 300);

        let (y_min, y_max) = data.calc_y_range();
        assert_eq!(y_min, 10.0);
        assert!((y_max - 55.0).abs() < 0.01); // 50 * 1.1
    }

    #[test]
    fn test_time_series_empty_range() {
        let data = TimeSeriesData::new();
        let (x_min, x_max) = data.calc_x_range();
        assert_eq!(x_min, 0);
        assert_eq!(x_max, 1);
        let (y_min, y_max) = data.calc_y_range();
        assert_eq!(y_min, 0.0);
        assert_eq!(y_max, 1.0);
    }

    #[test]
    fn test_worker_status_parsing() {
        assert_eq!(
            WorkerStatus::from_status_str("healthy"),
            WorkerStatus::Healthy
        );
        assert_eq!(
            WorkerStatus::from_status_str("RUNNING"),
            WorkerStatus::Healthy
        );
        assert_eq!(WorkerStatus::from_status_str("error"), WorkerStatus::Down);
        assert_eq!(
            WorkerStatus::from_status_str("unknown_status"),
            WorkerStatus::Unknown
        );
    }
}
