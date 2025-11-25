/// Query Performance Monitoring
///
/// Provides tools to track, analyze, and optimize database query performance.
/// Includes query plan analysis, metrics collection, and optimization recommendations.
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Performance metrics for a single query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMetrics {
    /// Query description or label
    pub query_name: String,
    /// Execution time in microseconds
    pub execution_time_us: u64,
    /// Number of rows returned
    pub rows_returned: Option<i64>,
    /// Whether query used an index
    pub used_index: bool,
    /// Query plan analysis
    pub query_plan: Option<String>,
    /// Timestamp of execution
    pub timestamp: String,
}

/// Aggregated performance statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryStats {
    /// Total number of executions
    pub execution_count: u64,
    /// Minimum execution time (microseconds)
    pub min_time_us: u64,
    /// Maximum execution time (microseconds)
    pub max_time_us: u64,
    /// Average execution time (microseconds)
    pub avg_time_us: u64,
    /// Total execution time (microseconds)
    pub total_time_us: u64,
    /// Percentile 95 execution time (microseconds)
    pub p95_time_us: u64,
    /// Percentile 99 execution time (microseconds)
    pub p99_time_us: u64,
    /// Whether queries used indexes
    pub index_usage_pct: f64,
    /// Recommendations for optimization
    pub recommendations: Vec<String>,
}

/// Performance monitor for tracking query metrics
pub struct QueryPerformanceMonitor {
    metrics: HashMap<String, Vec<QueryMetrics>>,
    slow_query_threshold_ms: u64,
}

impl QueryPerformanceMonitor {
    /// Create a new performance monitor
    pub fn new(slow_query_threshold_ms: u64) -> Self {
        Self {
            metrics: HashMap::new(),
            slow_query_threshold_ms,
        }
    }

    /// Record a query execution metric
    pub fn record(&mut self, metric: QueryMetrics) {
        if metric.execution_time_us / 1000 > self.slow_query_threshold_ms {
            warn!(
                query = %metric.query_name,
                time_ms = metric.execution_time_us / 1000,
                "Slow query detected"
            );
        }

        self.metrics
            .entry(metric.query_name.clone())
            .or_default()
            .push(metric);
    }

    /// Get statistics for a query
    pub fn get_stats(&self, query_name: &str) -> Option<QueryStats> {
        let metrics = self.metrics.get(query_name)?;
        if metrics.is_empty() {
            return None;
        }

        let execution_count = metrics.len() as u64;
        let times: Vec<u64> = metrics.iter().map(|m| m.execution_time_us).collect();

        let min_time_us = *times.iter().min().unwrap_or(&0);
        let max_time_us = *times.iter().max().unwrap_or(&0);
        let total_time_us: u64 = times.iter().sum();
        let avg_time_us = total_time_us / execution_count;

        // Calculate percentiles
        let mut sorted_times = times.clone();
        sorted_times.sort_unstable();
        let p95_idx = (sorted_times.len() * 95) / 100;
        let p99_idx = (sorted_times.len() * 99) / 100;
        let p95_time_us = sorted_times.get(p95_idx).copied().unwrap_or(0);
        let p99_time_us = sorted_times.get(p99_idx).copied().unwrap_or(0);

        // Calculate index usage percentage
        let index_used_count = metrics.iter().filter(|m| m.used_index).count() as f64;
        let index_usage_pct = (index_used_count / execution_count as f64) * 100.0;

        // Generate recommendations
        let recommendations = generate_recommendations(
            query_name,
            min_time_us,
            max_time_us,
            avg_time_us,
            index_usage_pct,
        );

        Some(QueryStats {
            execution_count,
            min_time_us,
            max_time_us,
            avg_time_us,
            total_time_us,
            p95_time_us,
            p99_time_us,
            index_usage_pct,
            recommendations,
        })
    }

    /// Get all recorded metrics
    pub fn all_metrics(&self) -> HashMap<String, QueryStats> {
        self.metrics
            .keys()
            .filter_map(|query_name| {
                self.get_stats(query_name)
                    .map(|stats| (query_name.clone(), stats))
            })
            .collect()
    }

    /// Clear all metrics
    pub fn clear(&mut self) {
        self.metrics.clear();
    }

    /// Generate a performance report
    pub fn report(&self) -> String {
        let mut report = String::from("=== Query Performance Report ===\n\n");

        let stats = self.all_metrics();
        if stats.is_empty() {
            report.push_str("No queries recorded yet.\n");
            return report;
        }

        for (query_name, stat) in stats {
            report.push_str(&format!("Query: {}\n", query_name));
            report.push_str(&format!("  Executions: {}\n", stat.execution_count));
            report.push_str(&format!(
                "  Time (min/avg/max): {:.2}ms / {:.2}ms / {:.2}ms\n",
                stat.min_time_us as f64 / 1000.0,
                stat.avg_time_us as f64 / 1000.0,
                stat.max_time_us as f64 / 1000.0
            ));
            report.push_str(&format!(
                "  P95/P99: {:.2}ms / {:.2}ms\n",
                stat.p95_time_us as f64 / 1000.0,
                stat.p99_time_us as f64 / 1000.0
            ));
            report.push_str(&format!("  Index Usage: {:.1}%\n", stat.index_usage_pct));

            if !stat.recommendations.is_empty() {
                report.push_str("  Recommendations:\n");
                for rec in &stat.recommendations {
                    report.push_str(&format!("    - {}\n", rec));
                }
            }
            report.push('\n');
        }

        report
    }
}

/// Generate optimization recommendations based on metrics
fn generate_recommendations(
    query_name: &str,
    min_time_us: u64,
    max_time_us: u64,
    avg_time_us: u64,
    index_usage_pct: f64,
) -> Vec<String> {
    let mut recommendations = Vec::new();

    // High variance suggests intermittent slow queries
    let variance = max_time_us as f64 - min_time_us as f64;
    if variance > (avg_time_us as f64 * 10.0) {
        recommendations.push(format!(
            "High variance in {} execution times ({:.0}% spread). Consider query caching.",
            query_name,
            (variance / avg_time_us as f64) * 100.0
        ));
    }

    // Low index usage suggests missing indexes
    if index_usage_pct < 50.0 {
        recommendations.push(format!(
            "{} uses indexes in only {:.1}% of executions. Consider adding indexes.",
            query_name, index_usage_pct
        ));
    }

    // High average execution time
    if avg_time_us > 50000 {
        // 50ms
        recommendations.push(format!(
            "{} averages {:.2}ms per execution. Consider optimization or pagination.",
            query_name,
            avg_time_us as f64 / 1000.0
        ));
    }

    recommendations
}

/// Helper to measure query execution time
pub async fn measure_query<F, T>(query_name: &str, f: F) -> Result<(T, Duration)>
where
    F: std::future::Future<Output = Result<T>>,
{
    let start = Instant::now();
    let result = f.await?;
    let elapsed = start.elapsed();

    debug!(
        query = query_name,
        duration_us = elapsed.as_micros(),
        "Query executed"
    );

    Ok((result, elapsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_stats_generation() {
        let mut monitor = QueryPerformanceMonitor::new(10); // 10ms threshold

        // Record some metrics
        for i in 0..10 {
            monitor.record(QueryMetrics {
                query_name: "test_query".to_string(),
                execution_time_us: 1000 + (i as u64 * 100),
                rows_returned: Some(100),
                used_index: i % 2 == 0,
                query_plan: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
        }

        let stats = monitor.get_stats("test_query").unwrap();
        assert_eq!(stats.execution_count, 10);
        assert_eq!(stats.min_time_us, 1000);
        assert_eq!(stats.max_time_us, 1900);
        assert!(stats.index_usage_pct > 40.0 && stats.index_usage_pct < 60.0); // ~50%
    }

    #[test]
    fn test_recommendations() {
        let recommendations = generate_recommendations("slow_query", 1000, 100000, 50000, 10.0);
        assert!(!recommendations.is_empty());
        assert!(recommendations.iter().any(|r| r.contains("High variance")));
        assert!(recommendations
            .iter()
            .any(|r| r.contains("Consider adding indexes")));
    }
}
