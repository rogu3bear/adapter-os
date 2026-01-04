//! SVG path building utilities for chart rendering.

use crate::components::charts::types::ChartPoint;
use crate::components::charts::utils::{InvertedYScale, TimeScale};

/// Build an SVG path string for a line chart.
///
/// Returns a path like "M x1,y1 L x2,y2 L x3,y3".
pub fn build_line_path(
    points: &[ChartPoint],
    x_scale: &TimeScale,
    y_scale: &InvertedYScale,
) -> String {
    if points.is_empty() {
        return String::new();
    }

    let mut path = String::with_capacity(points.len() * 20);

    for (i, point) in points.iter().enumerate() {
        let x = x_scale.scale(point.timestamp);
        let y = y_scale.scale(point.value);

        if i == 0 {
            path.push_str(&format!("M {:.1},{:.1}", x, y));
        } else {
            path.push_str(&format!(" L {:.1},{:.1}", x, y));
        }
    }

    path
}

/// Build an SVG path string for a sparkline (index-based X axis).
///
/// Maps points by index to fill the width, with Y mapped to height.
pub fn build_sparkline_path(values: &[f64], width: f64, height: f64, padding: f64) -> String {
    if values.is_empty() {
        return String::new();
    }

    // Calculate Y bounds
    let (y_min, y_max) = values.iter().fold((f64::MAX, f64::MIN), |(min, max), &v| {
        if v.is_finite() {
            (min.min(v), max.max(v))
        } else {
            (min, max)
        }
    });

    let y_range = if (y_max - y_min).abs() < f64::EPSILON {
        1.0
    } else {
        y_max - y_min
    };

    let chart_width = width - padding * 2.0;
    let chart_height = height - padding * 2.0;

    let x_step = if values.len() > 1 {
        chart_width / (values.len() - 1) as f64
    } else {
        0.0
    };

    let mut path = String::with_capacity(values.len() * 16);

    for (i, &value) in values.iter().enumerate() {
        let x = padding + i as f64 * x_step;
        let normalized_y = if y_range > 0.0 {
            (value - y_min) / y_range
        } else {
            0.5
        };
        // Invert Y: higher values should be at top (lower Y in SVG)
        let y = padding + (1.0 - normalized_y) * chart_height;

        if i == 0 {
            path.push_str(&format!("M {:.1},{:.1}", x, y));
        } else {
            path.push_str(&format!(" L {:.1},{:.1}", x, y));
        }
    }

    path
}

/// Build an SVG path for an area chart (line with fill to bottom).
pub fn build_area_path(values: &[f64], width: f64, height: f64, padding: f64) -> String {
    if values.is_empty() {
        return String::new();
    }

    let line_path = build_sparkline_path(values, width, height, padding);
    if line_path.is_empty() {
        return String::new();
    }

    let chart_width = width - padding * 2.0;
    let chart_height = height - padding * 2.0;
    let bottom_y = padding + chart_height;
    let start_x = padding;
    let end_x = padding + chart_width;

    // Close the path to form an area
    format!(
        "{} L {:.1},{:.1} L {:.1},{:.1} Z",
        line_path, end_x, bottom_y, start_x, bottom_y
    )
}

/// Build a smooth curve path using Catmull-Rom interpolation.
pub fn build_smooth_path(
    points: &[ChartPoint],
    x_scale: &TimeScale,
    y_scale: &InvertedYScale,
    tension: f64,
) -> String {
    if points.len() < 2 {
        return build_line_path(points, x_scale, y_scale);
    }

    let scaled: Vec<(f64, f64)> = points
        .iter()
        .map(|p| (x_scale.scale(p.timestamp), y_scale.scale(p.value)))
        .collect();

    let mut path = String::with_capacity(scaled.len() * 40);
    path.push_str(&format!("M {:.1},{:.1}", scaled[0].0, scaled[0].1));

    for i in 0..scaled.len() - 1 {
        let p0 = if i == 0 { scaled[0] } else { scaled[i - 1] };
        let p1 = scaled[i];
        let p2 = scaled[i + 1];
        let p3 = if i + 2 < scaled.len() {
            scaled[i + 2]
        } else {
            scaled[i + 1]
        };

        // Catmull-Rom to Bezier conversion
        let cp1x = p1.0 + (p2.0 - p0.0) * tension / 6.0;
        let cp1y = p1.1 + (p2.1 - p0.1) * tension / 6.0;
        let cp2x = p2.0 - (p3.0 - p1.0) * tension / 6.0;
        let cp2y = p2.1 - (p3.1 - p1.1) * tension / 6.0;

        path.push_str(&format!(
            " C {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
            cp1x, cp1y, cp2x, cp2y, p2.0, p2.1
        ));
    }

    path
}

/// Generate SVG circles for data points.
pub fn build_data_points_svg(
    points: &[ChartPoint],
    x_scale: &TimeScale,
    y_scale: &InvertedYScale,
    _radius: f64,
) -> Vec<(f64, f64, String)> {
    points
        .iter()
        .map(|p| {
            let x = x_scale.scale(p.timestamp);
            let y = y_scale.scale(p.value);
            let label = p.label.clone().unwrap_or_else(|| format!("{:.2}", p.value));
            (x, y, label)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_sparkline_path() {
        let values = vec![0.0, 50.0, 100.0];
        let path = build_sparkline_path(&values, 100.0, 50.0, 5.0);

        assert!(path.starts_with("M "));
        assert!(path.contains(" L "));
    }

    #[test]
    fn test_build_sparkline_path_empty() {
        let values: Vec<f64> = vec![];
        let path = build_sparkline_path(&values, 100.0, 50.0, 5.0);
        assert!(path.is_empty());
    }

    #[test]
    fn test_build_sparkline_path_single() {
        let values = vec![42.0];
        let path = build_sparkline_path(&values, 100.0, 50.0, 5.0);
        assert!(path.starts_with("M "));
        assert!(!path.contains(" L ")); // Single point has no line
    }

    #[test]
    fn test_build_area_path() {
        let values = vec![0.0, 50.0, 100.0];
        let path = build_area_path(&values, 100.0, 50.0, 5.0);

        assert!(path.starts_with("M "));
        assert!(path.ends_with(" Z")); // Closed path
    }
}
