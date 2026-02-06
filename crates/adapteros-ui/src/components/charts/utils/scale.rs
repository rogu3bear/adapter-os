//! Scale utilities for mapping data values to SVG coordinates.
//!
//! Provides linear and time-based scales for chart rendering.

/// A linear scale that maps a domain (data values) to a range (pixel coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearScale {
    domain_min: f64,
    domain_max: f64,
    range_min: f64,
    range_max: f64,
}

impl LinearScale {
    /// Create a new linear scale.
    ///
    /// # Arguments
    /// * `domain` - The input domain (min, max) in data units
    /// * `range` - The output range (min, max) in pixel units
    ///
    /// # Example
    /// ```
    /// let scale = LinearScale::new((0.0, 100.0), (0.0, 400.0));
    /// assert_eq!(scale.scale(50.0), 200.0);
    /// ```
    pub fn new(domain: (f64, f64), range: (f64, f64)) -> Self {
        let (domain_min, mut domain_max) = domain;
        let (range_min, range_max) = range;

        // Prevent division by zero
        if (domain_max - domain_min).abs() < f64::EPSILON {
            domain_max = domain_min + 1.0;
        }

        Self {
            domain_min,
            domain_max,
            range_min,
            range_max,
        }
    }

    /// Map a domain value to the range.
    pub fn scale(&self, value: f64) -> f64 {
        let normalized = (value - self.domain_min) / (self.domain_max - self.domain_min);
        self.range_min + normalized * (self.range_max - self.range_min)
    }

    /// Map a range value back to the domain (inverse).
    pub fn invert(&self, value: f64) -> f64 {
        let normalized = (value - self.range_min) / (self.range_max - self.range_min);
        self.domain_min + normalized * (self.domain_max - self.domain_min)
    }

    /// Generate evenly spaced tick values in the domain.
    pub fn ticks(&self, count: usize) -> Vec<f64> {
        if count == 0 {
            return Vec::new();
        }
        if count == 1 {
            return vec![self.domain_min];
        }

        let step = (self.domain_max - self.domain_min) / (count - 1) as f64;
        (0..count)
            .map(|i| self.domain_min + step * i as f64)
            .collect()
    }

    /// Generate nice tick values (rounded to sensible intervals).
    pub fn nice_ticks(&self, target_count: usize) -> Vec<f64> {
        if target_count == 0 {
            return Vec::new();
        }

        let range = self.domain_max - self.domain_min;
        let rough_step = range / target_count as f64;

        // Find a nice step size
        let magnitude = 10_f64.powf(rough_step.log10().floor());
        let residual = rough_step / magnitude;

        let nice_step = if residual <= 1.5 {
            magnitude
        } else if residual <= 3.0 {
            2.0 * magnitude
        } else if residual <= 7.0 {
            5.0 * magnitude
        } else {
            10.0 * magnitude
        };

        // Generate ticks
        let start = (self.domain_min / nice_step).ceil() * nice_step;
        let mut ticks = Vec::new();
        let mut value = start;

        while value <= self.domain_max + nice_step * 0.01 {
            ticks.push(value);
            value += nice_step;
        }

        ticks
    }

    /// Get the domain bounds.
    pub fn domain(&self) -> (f64, f64) {
        (self.domain_min, self.domain_max)
    }

    /// Get the range bounds.
    pub fn range(&self) -> (f64, f64) {
        (self.range_min, self.range_max)
    }
}

/// A scale for time values (Unix timestamps in milliseconds).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeScale {
    inner: LinearScale,
}

impl TimeScale {
    /// Create a new time scale.
    ///
    /// # Arguments
    /// * `time_range` - The input time range (min, max) in Unix milliseconds
    /// * `pixel_range` - The output range (min, max) in pixels
    pub fn new(time_range: (u64, u64), pixel_range: (f64, f64)) -> Self {
        Self {
            inner: LinearScale::new((time_range.0 as f64, time_range.1 as f64), pixel_range),
        }
    }

    /// Map a timestamp to pixel position.
    pub fn scale(&self, timestamp: u64) -> f64 {
        self.inner.scale(timestamp as f64)
    }

    /// Map a pixel position back to timestamp.
    pub fn invert(&self, pixel: f64) -> u64 {
        self.inner.invert(pixel) as u64
    }

    /// Generate tick timestamps (evenly spaced, not aligned to clock boundaries).
    pub fn ticks(&self, count: usize) -> Vec<u64> {
        self.inner
            .ticks(count)
            .into_iter()
            .map(|v| v as u64)
            .collect()
    }

    /// Generate nice time ticks aligned to sensible clock boundaries.
    ///
    /// Picks an interval (e.g. 5 min, 15 min, 1 hour) that gives close to
    /// `target_count` ticks, then aligns ticks to that interval from the epoch
    /// (midnight UTC), producing labels like "2 PM", "2:30 PM" instead of "2:17 PM".
    pub fn nice_ticks(&self, target_count: usize) -> Vec<u64> {
        if target_count == 0 {
            return Vec::new();
        }

        let (time_min, time_max) = self.time_range();
        let range_ms = time_max.saturating_sub(time_min);

        if range_ms == 0 {
            return vec![time_min];
        }

        // Sensible intervals in milliseconds
        const INTERVALS: &[u64] = &[
            1_000,          // 1 second
            5_000,          // 5 seconds
            15_000,         // 15 seconds
            30_000,         // 30 seconds
            60_000,         // 1 minute
            5 * 60_000,     // 5 minutes
            15 * 60_000,    // 15 minutes
            30 * 60_000,    // 30 minutes
            3_600_000,      // 1 hour
            2 * 3_600_000,  // 2 hours
            4 * 3_600_000,  // 4 hours
            6 * 3_600_000,  // 6 hours
            12 * 3_600_000, // 12 hours
            86_400_000,     // 1 day
        ];

        let ideal_interval = range_ms / target_count as u64;

        let interval = INTERVALS
            .iter()
            .copied()
            .min_by_key(|&i| (i as i64 - ideal_interval as i64).unsigned_abs())
            .unwrap_or(ideal_interval)
            .max(1); // prevent division by zero

        // Align first tick to interval boundary (from epoch = midnight UTC)
        let first_tick = ((time_min + interval - 1) / interval) * interval;

        let mut ticks = Vec::new();
        let mut t = first_tick;
        while t <= time_max {
            ticks.push(t);
            t += interval;
        }

        // Fall back to even spacing if alignment produced too few ticks
        if ticks.len() < 2 {
            return self.ticks(target_count);
        }

        ticks
    }

    /// Get the time range.
    pub fn time_range(&self) -> (u64, u64) {
        let (min, max) = self.inner.domain();
        (min as u64, max as u64)
    }

    /// Get the pixel range.
    pub fn pixel_range(&self) -> (f64, f64) {
        self.inner.range()
    }

    /// Access the underlying linear scale.
    pub fn inner(&self) -> &LinearScale {
        &self.inner
    }
}

/// Scale for Y-axis that inverts coordinates (SVG Y grows downward).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InvertedYScale {
    inner: LinearScale,
}

impl InvertedYScale {
    /// Create an inverted Y scale.
    ///
    /// Maps data values to SVG Y coordinates (inverted: higher values = lower Y).
    ///
    /// # Arguments
    /// * `domain` - Data range (min, max)
    /// * `range` - SVG Y range (top, bottom) - typically (padding, height - padding)
    pub fn new(domain: (f64, f64), range: (f64, f64)) -> Self {
        // Swap range to invert
        Self {
            inner: LinearScale::new(domain, (range.1, range.0)),
        }
    }

    /// Map a data value to SVG Y coordinate.
    pub fn scale(&self, value: f64) -> f64 {
        self.inner.scale(value)
    }

    /// Map an SVG Y coordinate back to data value.
    pub fn invert(&self, y: f64) -> f64 {
        self.inner.invert(y)
    }

    /// Generate tick values.
    pub fn ticks(&self, count: usize) -> Vec<f64> {
        self.inner.ticks(count)
    }

    /// Generate nice tick values.
    pub fn nice_ticks(&self, count: usize) -> Vec<f64> {
        self.inner.nice_ticks(count)
    }

    /// Get the domain bounds.
    pub fn domain(&self) -> (f64, f64) {
        self.inner.domain()
    }
}

// ============================================================================
// Range Calculation Utilities
// ============================================================================

/// Configuration for Y-axis range calculation.
#[derive(Debug, Clone, Copy)]
pub struct RangeConfig {
    /// Minimum value padding factor (0.0 = no padding, 0.1 = 10% padding below).
    pub min_padding: f64,
    /// Maximum value padding factor (0.0 = no padding, 0.1 = 10% padding above).
    pub max_padding: f64,
    /// If true, include zero in the range even if all data is positive.
    pub include_zero: bool,
    /// Optional fixed minimum (overrides data-derived min).
    pub fixed_min: Option<f64>,
    /// Optional fixed maximum (overrides data-derived max).
    pub fixed_max: Option<f64>,
}

impl Default for RangeConfig {
    fn default() -> Self {
        Self {
            min_padding: 0.0,
            max_padding: 0.1,
            include_zero: false,
            fixed_min: None,
            fixed_max: None,
        }
    }
}

impl RangeConfig {
    /// Preset for percentage values (0-100 scale).
    pub fn percent() -> Self {
        Self {
            min_padding: 0.0,
            max_padding: 0.0,
            include_zero: true,
            fixed_min: Some(0.0),
            fixed_max: Some(100.0),
        }
    }

    /// Preset for values that should include zero.
    pub fn zero_based() -> Self {
        Self {
            include_zero: true,
            ..Default::default()
        }
    }

    /// Preset with symmetric padding.
    pub fn with_padding(padding: f64) -> Self {
        Self {
            min_padding: padding,
            max_padding: padding,
            ..Default::default()
        }
    }
}

/// Calculate Y range from data with configurable options.
pub fn calc_range_with_config(
    values: impl Iterator<Item = f64>,
    config: &RangeConfig,
) -> (f64, f64) {
    let mut data_min: Option<f64> = None;
    let mut data_max: Option<f64> = None;

    for value in values {
        if value.is_finite() {
            data_min = Some(match data_min {
                Some(min) => min.min(value),
                None => value,
            });
            data_max = Some(match data_max {
                Some(max) => max.max(value),
                None => value,
            });
        }
    }

    let mut y_min = config.fixed_min.or(data_min).unwrap_or(0.0);
    let mut y_max = config.fixed_max.or(data_max).unwrap_or(y_min);

    // Include zero if configured
    if config.include_zero {
        if y_min > 0.0 && config.fixed_min.is_none() {
            y_min = 0.0;
        }
        if y_max < 0.0 && config.fixed_max.is_none() {
            y_max = 0.0;
        }
    }

    // Ensure non-zero range
    if (y_max - y_min).abs() < f64::EPSILON {
        y_max = y_min + 1.0;
    }

    // Apply padding (only if not using fixed bounds)
    let range = y_max - y_min;
    if config.fixed_min.is_none() {
        y_min -= range * config.min_padding;
    }
    if config.fixed_max.is_none() {
        y_max += range * config.max_padding;
    }

    (y_min, y_max)
}

/// Merge multiple value iterators into a single range.
pub fn merge_ranges<I, It>(iterators: I, config: &RangeConfig) -> (f64, f64)
where
    I: IntoIterator<Item = It>,
    It: Iterator<Item = f64>,
{
    let combined = iterators.into_iter().flatten();
    calc_range_with_config(combined, config)
}

/// Round a range to "nice" boundaries for axis display.
pub fn nice_range(min: f64, max: f64) -> (f64, f64) {
    let range = max - min;
    if range.abs() < f64::EPSILON {
        return (min, min + 1.0);
    }

    let magnitude = 10_f64.powf(range.log10().floor());
    let residual = range / magnitude;

    let nice_range = if residual <= 1.5 {
        magnitude
    } else if residual <= 3.0 {
        2.0 * magnitude
    } else if residual <= 7.0 {
        5.0 * magnitude
    } else {
        10.0 * magnitude
    };

    let nice_min = (min / nice_range).floor() * nice_range;
    let nice_max = (max / nice_range).ceil() * nice_range;

    (nice_min, nice_max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_scale_basic() {
        let scale = LinearScale::new((0.0, 100.0), (0.0, 400.0));
        assert!((scale.scale(0.0) - 0.0).abs() < f64::EPSILON);
        assert!((scale.scale(50.0) - 200.0).abs() < f64::EPSILON);
        assert!((scale.scale(100.0) - 400.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_linear_scale_with_offset() {
        let scale = LinearScale::new((10.0, 20.0), (100.0, 200.0));
        assert!((scale.scale(10.0) - 100.0).abs() < f64::EPSILON);
        assert!((scale.scale(15.0) - 150.0).abs() < f64::EPSILON);
        assert!((scale.scale(20.0) - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_linear_scale_invert() {
        let scale = LinearScale::new((0.0, 100.0), (0.0, 400.0));
        assert!((scale.invert(200.0) - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_linear_scale_zero_domain() {
        // Should handle zero-width domain gracefully
        let scale = LinearScale::new((50.0, 50.0), (0.0, 100.0));
        let result = scale.scale(50.0);
        assert!(result.is_finite());
    }

    #[test]
    fn test_linear_scale_ticks() {
        let scale = LinearScale::new((0.0, 100.0), (0.0, 400.0));
        let ticks = scale.ticks(5);
        assert_eq!(ticks.len(), 5);
        assert!((ticks[0] - 0.0).abs() < f64::EPSILON);
        assert!((ticks[4] - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_time_scale() {
        let scale = TimeScale::new((1000, 2000), (0.0, 100.0));
        assert!((scale.scale(1000) - 0.0).abs() < f64::EPSILON);
        assert!((scale.scale(1500) - 50.0).abs() < f64::EPSILON);
        assert!((scale.scale(2000) - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_inverted_y_scale() {
        let scale = InvertedYScale::new((0.0, 100.0), (20.0, 180.0));
        // Higher values should map to lower Y (closer to top)
        let y_at_0 = scale.scale(0.0);
        let y_at_100 = scale.scale(100.0);
        assert!(y_at_100 < y_at_0); // 100 should be higher up (lower Y)
    }

    // Range utility tests

    #[test]
    fn test_range_config_percent() {
        let config = RangeConfig::percent();
        let (min, max) = calc_range_with_config([25.0, 75.0].into_iter(), &config);
        assert_eq!(min, 0.0);
        assert_eq!(max, 100.0);
    }

    #[test]
    fn test_range_config_zero_based() {
        let config = RangeConfig::zero_based();
        let (min, max) = calc_range_with_config([10.0, 50.0].into_iter(), &config);
        assert_eq!(min, 0.0);
        // 50 + 10% padding = 55
        assert!((max - 55.0).abs() < 0.1);
    }

    #[test]
    fn test_range_config_default_padding() {
        let config = RangeConfig::default();
        let (min, max) = calc_range_with_config([10.0, 20.0].into_iter(), &config);
        assert_eq!(min, 10.0); // no min padding by default
                               // 20 + 10% of range (10) = 21
        assert!((max - 21.0).abs() < 0.1);
    }

    #[test]
    fn test_merge_ranges() {
        let config = RangeConfig::default();
        let series1 = vec![10.0, 20.0, 30.0];
        let series2 = vec![5.0, 15.0, 25.0];
        let (min, max) = merge_ranges([series1.into_iter(), series2.into_iter()], &config);
        assert_eq!(min, 5.0);
        // 30 + 10% of 25 = 32.5
        assert!((max - 32.5).abs() < 0.1);
    }

    #[test]
    fn test_nice_range() {
        let (min, max) = nice_range(3.2, 47.8);
        assert_eq!(min, 0.0);
        assert_eq!(max, 50.0);
    }

    #[test]
    fn test_nice_range_small() {
        let (min, max) = nice_range(0.5, 2.3);
        assert_eq!(min, 0.0);
        // range=1.8, magnitude=1.0, residual=1.8 -> nice_range=2.0 -> ceil(2.3/2)*2=4
        assert!((max - 4.0).abs() < 0.1);
    }

    #[test]
    fn test_calc_range_empty() {
        let config = RangeConfig::default();
        let (min, max) = calc_range_with_config(std::iter::empty(), &config);
        assert_eq!(min, 0.0);
        // After ensuring non-zero range (0, 1), 10% max padding is applied: 1 + 0.1 = 1.1
        assert!((max - 1.1).abs() < 0.01);
    }
}
