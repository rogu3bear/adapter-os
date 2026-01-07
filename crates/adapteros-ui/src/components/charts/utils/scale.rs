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

    /// Generate tick timestamps.
    pub fn ticks(&self, count: usize) -> Vec<u64> {
        self.inner
            .ticks(count)
            .into_iter()
            .map(|v| v as u64)
            .collect()
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
}
