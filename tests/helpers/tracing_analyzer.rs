use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{span, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{Layer, Registry};

/// Captured span information with timing data
#[derive(Debug, Clone)]
pub struct SpanRecord {
    pub name: String,
    pub duration_ms: u64,
    pub fields: HashMap<String, String>,
}

/// Custom tracing layer that captures span events
#[derive(Clone)]
pub struct TracingCapture {
    spans: Arc<Mutex<Vec<SpanRecord>>>,
}

impl TracingCapture {
    pub fn new() -> Self {
        Self {
            spans: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn get_spans(&self) -> Vec<SpanRecord> {
        self.spans.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.spans.lock().unwrap().clear();
    }

    /// Get spans matching a name pattern
    pub fn spans_by_name(&self, pattern: &str) -> Vec<SpanRecord> {
        self.spans
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.name.contains(pattern))
            .cloned()
            .collect()
    }

    /// Get total duration for spans matching a pattern
    pub fn total_duration_ms(&self, pattern: &str) -> u64 {
        self.spans_by_name(pattern)
            .iter()
            .map(|s| s.duration_ms)
            .sum()
    }
}

impl Default for TracingCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Layer<S> for TracingCapture
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_enter(&self, id: &span::Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let mut extensions = span.extensions_mut();
            extensions.insert(std::time::Instant::now());
        }
    }

    fn on_close(&self, id: span::Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(&id) {
            let elapsed = {
                let extensions = span.extensions();
                extensions
                    .get::<std::time::Instant>()
                    .map(|start| start.elapsed())
            };

            if let Some(elapsed) = elapsed {
                let name = span.name().to_string();
                let duration_ms = elapsed.as_millis() as u64;

                // Extract fields from the span
                let mut fields = HashMap::new();
                // Note: Field extraction is limited in this simple implementation
                // For production use, consider using tracing_subscriber::fmt::format::FmtContext

                let record = SpanRecord {
                    name,
                    duration_ms,
                    fields,
                };

                self.spans.lock().unwrap().push(record);
            }
        }
    }
}

/// Performance metrics extracted from tracing spans
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingMetrics {
    /// Total time in filesystem operations (ms)
    pub filesystem_time_ms: u64,
    /// Total time in database operations (ms)
    pub database_time_ms: u64,
    /// Total handler execution time (ms)
    pub total_handler_time_ms: u64,
    /// Ratio of filesystem to database time
    pub fs_db_ratio: f64,
    /// Individual span breakdown
    pub span_breakdown: HashMap<String, u64>,
}

impl TimingMetrics {
    /// Create metrics from captured spans
    pub fn from_spans(spans: &[SpanRecord]) -> Self {
        let mut span_breakdown = HashMap::new();

        // Calculate filesystem time (sum of blocking operation spans)
        let filesystem_spans = [
            "directory_adapter_blocking_ops",
            "path_validation",
            "directory_analysis",
            "artifact_creation",
        ];
        let filesystem_time_ms: u64 = spans
            .iter()
            .filter(|s| filesystem_spans.iter().any(|pattern| s.name.contains(pattern)))
            .map(|s| {
                span_breakdown.insert(s.name.clone(), s.duration_ms);
                s.duration_ms
            })
            .sum();

        // Calculate database time (sum of db_* spans)
        let database_time_ms: u64 = spans
            .iter()
            .filter(|s| s.name.starts_with("db_"))
            .map(|s| {
                span_breakdown.insert(s.name.clone(), s.duration_ms);
                s.duration_ms
            })
            .sum();

        // Get total handler time
        let total_handler_time_ms = spans
            .iter()
            .find(|s| s.name == "upsert_directory_adapter_handler")
            .map(|s| s.duration_ms)
            .unwrap_or(filesystem_time_ms + database_time_ms);

        // Calculate ratio (avoid division by zero)
        let fs_db_ratio = if database_time_ms > 0 {
            filesystem_time_ms as f64 / database_time_ms as f64
        } else {
            f64::INFINITY
        };

        Self {
            filesystem_time_ms,
            database_time_ms,
            total_handler_time_ms,
            fs_db_ratio,
            span_breakdown,
        }
    }

    /// Save metrics to JSON file
    pub fn save_baseline<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(self)?;

        // Ensure parent directory exists
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(path, json)?;
        Ok(())
    }

    /// Load metrics from JSON file
    pub fn load_baseline<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let metrics = serde_json::from_str(&json)?;
        Ok(metrics)
    }
}

/// Report comparing baseline and current metrics
#[derive(Debug, Clone)]
pub struct ImprovementReport {
    pub baseline: TimingMetrics,
    pub current: TimingMetrics,
    pub total_time_improvement_pct: f64,
    pub filesystem_time_improvement_pct: f64,
    pub database_time_improvement_pct: f64,
    pub ratio_improvement: f64,
}

impl ImprovementReport {
    /// Compare current metrics against baseline
    pub fn compare(baseline: TimingMetrics, current: TimingMetrics) -> Self {
        let total_time_improvement_pct = if baseline.total_handler_time_ms > 0 {
            ((baseline.total_handler_time_ms as f64 - current.total_handler_time_ms as f64)
                / baseline.total_handler_time_ms as f64)
                * 100.0
        } else {
            0.0
        };

        let filesystem_time_improvement_pct = if baseline.filesystem_time_ms > 0 {
            ((baseline.filesystem_time_ms as f64 - current.filesystem_time_ms as f64)
                / baseline.filesystem_time_ms as f64)
                * 100.0
        } else {
            0.0
        };

        let database_time_improvement_pct = if baseline.database_time_ms > 0 {
            ((baseline.database_time_ms as f64 - current.database_time_ms as f64)
                / baseline.database_time_ms as f64)
                * 100.0
        } else {
            0.0
        };

        // Ratio improvement: positive means ratio is closer to 1.0 (more parallel)
        let ratio_improvement = baseline.fs_db_ratio - current.fs_db_ratio;

        Self {
            baseline,
            current,
            total_time_improvement_pct,
            filesystem_time_improvement_pct,
            database_time_improvement_pct,
            ratio_improvement,
        }
    }

    /// Assert that improvements meet expected thresholds
    pub fn assert_improvements(
        &self,
        min_total_improvement_pct: f64,
        max_acceptable_ratio: f64,
    ) {
        assert!(
            self.total_time_improvement_pct >= min_total_improvement_pct,
            "Total time improvement {}% is below threshold {}%",
            self.total_time_improvement_pct,
            min_total_improvement_pct
        );

        assert!(
            self.current.fs_db_ratio <= max_acceptable_ratio,
            "Current FS/DB ratio {:.2} exceeds max acceptable ratio {:.2}",
            self.current.fs_db_ratio,
            max_acceptable_ratio
        );
    }

    /// Print detailed comparison report
    pub fn print_report(&self) {
        println!("\n=== Performance Comparison Report ===");
        println!("\nBaseline:");
        println!("  Total time:      {} ms", self.baseline.total_handler_time_ms);
        println!("  Filesystem time: {} ms", self.baseline.filesystem_time_ms);
        println!("  Database time:   {} ms", self.baseline.database_time_ms);
        println!("  FS/DB ratio:     {:.2}", self.baseline.fs_db_ratio);

        println!("\nCurrent:");
        println!("  Total time:      {} ms", self.current.total_handler_time_ms);
        println!("  Filesystem time: {} ms", self.current.filesystem_time_ms);
        println!("  Database time:   {} ms", self.current.database_time_ms);
        println!("  FS/DB ratio:     {:.2}", self.current.fs_db_ratio);

        println!("\nImprovements:");
        println!("  Total time:      {:.1}%", self.total_time_improvement_pct);
        println!("  Filesystem time: {:.1}%", self.filesystem_time_improvement_pct);
        println!("  Database time:   {:.1}%", self.database_time_improvement_pct);
        println!("  Ratio change:    {:.2}", self.ratio_improvement);

        println!("\nSpan Breakdown:");
        for (span_name, duration) in &self.current.span_breakdown {
            let baseline_duration = self.baseline.span_breakdown.get(span_name).unwrap_or(&0);
            let change = if *baseline_duration > 0 {
                (((*baseline_duration as f64) - (*duration as f64)) / (*baseline_duration as f64)) * 100.0
            } else {
                0.0
            };
            println!("  {:40} {:6} ms ({:+.1}%)", span_name, duration, change);
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_metrics_calculation() {
        let spans = vec![
            SpanRecord {
                name: "upsert_directory_adapter_handler".to_string(),
                duration_ms: 600,
                fields: HashMap::new(),
            },
            SpanRecord {
                name: "directory_adapter_blocking_ops".to_string(),
                duration_ms: 500,
                fields: HashMap::new(),
            },
            SpanRecord {
                name: "db_get_adapter_check".to_string(),
                duration_ms: 50,
                fields: HashMap::new(),
            },
            SpanRecord {
                name: "db_register_adapter".to_string(),
                duration_ms: 30,
                fields: HashMap::new(),
            },
        ];

        let metrics = TimingMetrics::from_spans(&spans);

        assert_eq!(metrics.filesystem_time_ms, 500);
        assert_eq!(metrics.database_time_ms, 80);
        assert_eq!(metrics.total_handler_time_ms, 600);
        assert!((metrics.fs_db_ratio - 6.25).abs() < 0.01);
    }

    #[test]
    fn test_improvement_report() {
        let baseline = TimingMetrics {
            filesystem_time_ms: 500,
            database_time_ms: 50,
            total_handler_time_ms: 550,
            fs_db_ratio: 10.0,
            span_breakdown: HashMap::new(),
        };

        let current = TimingMetrics {
            filesystem_time_ms: 500,
            database_time_ms: 50,
            total_handler_time_ms: 500, // Improved by parallelization
            fs_db_ratio: 1.0,           // Much better ratio
            span_breakdown: HashMap::new(),
        };

        let report = ImprovementReport::compare(baseline, current);

        // Should show ~9% improvement in total time (550 -> 500)
        assert!((report.total_time_improvement_pct - 9.09).abs() < 0.1);

        // Should show significant ratio improvement
        assert!((report.ratio_improvement - 9.0).abs() < 0.1);
    }
}
