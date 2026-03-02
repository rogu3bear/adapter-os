use crate::Db;
/// Query Performance Monitoring
///
/// Provides tools to track, analyze, and optimize database query performance.
/// Includes query plan analysis, metrics collection, and optimization recommendations.
use adapteros_core::{AosError, Result};
use chrono;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio;
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
    /// Tenant that executed the query (if tenant-scoped)
    pub tenant_id: Option<String>,
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

/// Multi-tenant configuration for dataset generation
#[derive(Debug, Clone)]
pub struct MultiTenantConfig {
    /// Number of tenants to create
    pub tenant_count: usize,
    /// Number of adapters per tenant (base count)
    pub adapters_per_tenant_base: usize,
    /// Variance in adapter count per tenant (± percentage)
    pub adapter_count_variance_pct: f64,
    /// Active/inactive adapter ratio (0.0-1.0, percentage active)
    pub active_adapter_ratio: f64,
    /// Tier distribution weights
    pub tier_weights: HashMap<String, f64>,
    /// Creation date range in days
    pub creation_date_range_days: u32,
}

/// Concurrency monitoring configuration
#[derive(Debug, Clone)]
pub struct ConcurrencyConfig {
    /// Number of concurrent workers
    pub concurrent_workers: usize,
    /// Duration of concurrency test in milliseconds
    pub test_duration_ms: u64,
    /// Query rate per second per worker
    pub queries_per_second_per_worker: usize,
}

/// Performance regression thresholds
#[derive(Debug, Clone)]
pub struct RegressionThresholds {
    /// Maximum allowed P95 latency in milliseconds
    pub max_p95_latency_ms: u64,
    /// Maximum allowed P99 latency in milliseconds
    pub max_p99_latency_ms: u64,
    /// Minimum required index usage percentage
    pub min_index_usage_pct: f64,
    /// Maximum allowed query time variance coefficient
    pub max_variance_coefficient: f64,
    /// Minimum performance improvement percentage for migrations
    pub min_improvement_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TenantQueryKey {
    tenant_id: Option<String>,
    query_name: String,
}

impl TenantQueryKey {
    fn from_metric(metric: &QueryMetrics) -> Self {
        Self {
            tenant_id: metric.tenant_id.clone(),
            query_name: metric.query_name.clone(),
        }
    }

    fn from_parts(tenant_id: Option<&str>, query_name: &str) -> Self {
        Self {
            tenant_id: tenant_id.map(|t| t.to_string()),
            query_name: query_name.to_string(),
        }
    }

    fn tenant_label(&self) -> Option<&str> {
        self.tenant_id.as_deref()
    }
}

/// Performance monitor for tracking query metrics
#[derive(Clone)]
pub struct QueryPerformanceMonitor {
    metrics: HashMap<String, Vec<QueryMetrics>>,
    slow_query_threshold_ms: u64,
    /// Multi-tenant configuration for dataset generation
    multi_tenant_config: Option<MultiTenantConfig>,
    /// Concurrency configuration for performance testing
    concurrency_config: Option<ConcurrencyConfig>,
    /// Regression thresholds for alerting
    regression_thresholds: RegressionThresholds,
    /// Concurrency metrics collected during concurrent testing
    concurrency_metrics: HashMap<String, Vec<ConcurrencyMetric>>,
    /// Tenant-scoped metrics for per-tenant visibility
    tenant_metrics: HashMap<TenantQueryKey, Vec<QueryMetrics>>,
    /// Stored baseline stats for optimization impact tracking
    baseline_stats: HashMap<TenantQueryKey, QueryStats>,
}

/// Concurrency performance metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyMetric {
    /// Query name
    pub query_name: String,
    /// Worker ID that executed the query
    pub worker_id: usize,
    /// Execution time in microseconds
    pub execution_time_us: u64,
    /// Whether query used an index
    pub used_index: bool,
    /// Timestamp of execution
    pub timestamp: String,
    /// Memory usage in bytes (if available)
    pub memory_usage_bytes: Option<u64>,
}

/// Configuration for multi-tenant dataset generation
#[derive(Debug)]
pub struct MultiTenantDataset {
    /// Tenant IDs created
    pub tenant_ids: Vec<String>,
    /// Total adapters across all tenants
    pub total_adapters: usize,
    /// Adapters per tenant
    pub adapters_per_tenant: usize,
}

/// Adapter distribution parameters for realistic data
#[derive(Debug)]
pub struct AdapterDistribution {
    /// Tier distribution weights (tier -> probability)
    pub tier_weights: HashMap<String, f64>,
    /// State distribution weights (state -> probability)
    pub state_weights: HashMap<String, f64>,
    /// Creation date range in days
    pub creation_date_range_days: u32,
}

/// Analysis of query plan performance changes
#[derive(Debug)]
pub struct QueryPlanAnalysis {
    /// Tenant ID (optional)
    pub tenant_id: Option<String>,
    /// Query plan before migration
    pub before_plan: String,
    /// Query plan after migration
    pub after_plan: String,
    /// Execution time before migration (microseconds)
    pub before_time_us: u64,
    /// Execution time after migration (microseconds)
    pub after_time_us: u64,
    /// Performance improvement percentage
    pub improvement_percentage: f64,
    /// Whether index was utilized before migration
    pub index_utilization_before: bool,
    /// Whether index was utilized after migration
    pub index_utilization_after: bool,
    /// Whether 50% improvement target was met
    pub target_met: bool,
}

/// Impact of an optimization change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationImpact {
    pub query_name: String,
    pub tenant_id: Option<String>,
    pub baseline_latency_us: u64,
    pub current_latency_us: u64,
    pub improvement_pct: f64,
    pub roi_score: f64, // improvement * calls per day
    pub timestamp: String,
}

impl OptimizationImpact {
    fn from_stats(
        key: &TenantQueryKey,
        baseline: &QueryStats,
        current: &QueryStats,
        _min_improvement_pct: f64,
    ) -> Self {
        let baseline_latency = baseline.avg_time_us;
        let current_latency = current.avg_time_us;

        let improvement_pct = if baseline_latency > 0 {
            ((baseline_latency as f64 - current_latency as f64) / baseline_latency as f64) * 100.0
        } else {
            0.0
        };

        // Simple ROI score: improvement_ms * execution_count
        let improvement_ms = (baseline_latency.saturating_sub(current_latency)) as f64 / 1000.0;
        let roi_score = improvement_ms * current.execution_count as f64;

        Self {
            query_name: key.query_name.clone(),
            tenant_id: key.tenant_label().map(|t| t.to_string()),
            baseline_latency_us: baseline_latency,
            current_latency_us: current_latency,
            improvement_pct,
            roi_score,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Performance regression alert
#[derive(Debug)]
pub struct RegressionAlert {
    /// Query name that regressed
    pub query_name: String,
    /// Actual latency in milliseconds
    pub latency_ms: u64,
    /// Threshold latency in milliseconds
    pub threshold_ms: u64,
    /// Tenant ID if tenant-scoped
    pub tenant_id: Option<String>,
    /// Timestamp of alert
    pub timestamp: String,
}

/// Validation result for performance targets
#[derive(Debug)]
pub enum ValidationResult {
    /// Performance target met
    Passed { improvement_pct: f64 },
    /// Performance target not met
    Failed {
        actual_improvement: f64,
        required_improvement: f64,
    },
    /// Cannot determine if target met
    Inconclusive { reason: String },
}

/// Statistical analysis of performance variance
#[derive(Debug, Clone)]
pub struct PerformanceVarianceAnalysis {
    /// Query name
    pub query_name: String,
    /// Sample size
    pub sample_size: usize,
    /// Mean execution time in microseconds
    pub mean_time_us: f64,
    /// Variance in microseconds squared
    pub variance_us2: f64,
    /// Standard deviation in microseconds
    pub std_dev_us: f64,
    /// Coefficient of variation (std_dev / mean)
    pub coefficient_of_variation: f64,
    /// 95% confidence interval lower bound
    pub confidence_interval_95_lower: f64,
    /// 95% confidence interval upper bound
    pub confidence_interval_95_upper: f64,
    /// Whether variance exceeds acceptable threshold
    pub has_significant_variance: bool,
}

/// Threshold violation types
#[derive(Debug, Clone)]
pub enum ViolationType {
    /// P95 latency exceeded threshold
    P95LatencyExceeded { actual_ms: u64, threshold_ms: u64 },
    /// P99 latency exceeded threshold
    P99LatencyExceeded { actual_ms: u64, threshold_ms: u64 },
    /// Index usage below minimum threshold
    LowIndexUsage { actual_pct: f64, threshold_pct: f64 },
    /// Query variance coefficient too high
    HighVariance { coefficient: f64, threshold: f64 },
}

/// Severity levels for violations
#[derive(Debug, Clone, PartialEq)]
pub enum ViolationSeverity {
    /// Warning level violation
    Warning,
    /// Critical level violation requiring immediate attention
    Critical,
}

/// Threshold violation report
#[derive(Debug, Clone)]
pub struct ThresholdViolation {
    /// Query name that violated threshold
    pub query_name: String,
    /// Type of violation
    pub violation_type: ViolationType,
    /// Severity level
    pub severity: ViolationSeverity,
    /// Tenant associated with the violation (if any)
    pub tenant_id: Option<String>,
    /// Timestamp of violation detection
    pub timestamp: String,
}

impl QueryPerformanceMonitor {
    /// Create a new performance monitor
    pub fn new(slow_query_threshold_ms: u64) -> Self {
        Self::with_config(slow_query_threshold_ms, None, None, None)
    }

    /// Create a new performance monitor with full configuration
    pub fn with_config(
        slow_query_threshold_ms: u64,
        multi_tenant_config: Option<MultiTenantConfig>,
        concurrency_config: Option<ConcurrencyConfig>,
        regression_thresholds: Option<RegressionThresholds>,
    ) -> Self {
        Self {
            metrics: HashMap::new(),
            slow_query_threshold_ms,
            multi_tenant_config,
            concurrency_config,
            regression_thresholds: regression_thresholds.unwrap_or(RegressionThresholds {
                max_p95_latency_ms: 100,       // 100ms default
                max_p99_latency_ms: 200,       // 200ms default
                min_index_usage_pct: 90.0,     // 90% default
                max_variance_coefficient: 5.0, // 5x default
                min_improvement_pct: 50.0,     // 50% default
            }),
            concurrency_metrics: HashMap::new(),
            tenant_metrics: HashMap::new(),
            baseline_stats: HashMap::new(),
        }
    }

    /// Get multi-tenant configuration
    pub fn multi_tenant_config(&self) -> Option<&MultiTenantConfig> {
        self.multi_tenant_config.as_ref()
    }

    /// Set multi-tenant configuration
    pub fn set_multi_tenant_config(&mut self, config: MultiTenantConfig) {
        self.multi_tenant_config = Some(config);
    }

    fn compute_stats(query_name: &str, metrics: &[QueryMetrics]) -> QueryStats {
        let execution_count = metrics.len() as u64;
        let mut times: Vec<u64> = metrics.iter().map(|m| m.execution_time_us).collect();

        let min_time_us = *times.iter().min().unwrap_or(&0);
        let max_time_us = *times.iter().max().unwrap_or(&0);
        let total_time_us: u64 = times.iter().sum();
        let avg_time_us = if execution_count > 0 {
            total_time_us / execution_count
        } else {
            0
        };

        times.sort_unstable();
        let p95_idx = (times.len() * 95) / 100;
        let p99_idx = (times.len() * 99) / 100;
        let p95_time_us = times.get(p95_idx).copied().unwrap_or(0);
        let p99_time_us = times.get(p99_idx).copied().unwrap_or(0);

        let index_used_count = metrics.iter().filter(|m| m.used_index).count() as f64;
        let index_usage_pct = if execution_count > 0 {
            (index_used_count / execution_count as f64) * 100.0
        } else {
            0.0
        };

        let recommendations = generate_recommendations(
            query_name,
            min_time_us,
            max_time_us,
            avg_time_us,
            index_usage_pct,
        );

        QueryStats {
            execution_count,
            min_time_us,
            max_time_us,
            avg_time_us,
            total_time_us,
            p95_time_us,
            p99_time_us,
            index_usage_pct,
            recommendations,
        }
    }

    /// Get concurrency configuration
    pub fn concurrency_config(&self) -> Option<&ConcurrencyConfig> {
        self.concurrency_config.as_ref()
    }

    /// Set concurrency configuration
    pub fn set_concurrency_config(&mut self, config: ConcurrencyConfig) {
        self.concurrency_config = Some(config);
    }

    /// Get regression thresholds
    pub fn regression_thresholds(&self) -> &RegressionThresholds {
        &self.regression_thresholds
    }

    /// Set regression thresholds
    pub fn set_regression_thresholds(&mut self, thresholds: RegressionThresholds) {
        self.regression_thresholds = thresholds;
    }

    /// Record a query execution metric
    pub fn record(&mut self, metric: QueryMetrics) {
        if metric.execution_time_us / 1000 > self.slow_query_threshold_ms {
            warn!(
                query = %metric.query_name,
                tenant = metric.tenant_id.as_deref().unwrap_or("system"),
                time_ms = metric.execution_time_us / 1000,
                "Slow query detected"
            );
        }

        let metric_clone = metric.clone();

        self.metrics
            .entry(metric.query_name.clone())
            .or_default()
            .push(metric);

        self.tenant_metrics
            .entry(TenantQueryKey::from_metric(&metric_clone))
            .or_default()
            .push(metric_clone);
    }

    /// Get statistics for a query
    pub fn get_stats(&self, query_name: &str) -> Option<QueryStats> {
        let metrics = self.metrics.get(query_name)?;
        if metrics.is_empty() {
            return None;
        }

        Some(Self::compute_stats(query_name, metrics))
    }

    /// Get statistics scoped to a tenant (or global if tenant_id is None)
    pub fn get_tenant_stats(
        &self,
        tenant_id: Option<&str>,
        query_name: &str,
    ) -> Option<QueryStats> {
        let key = TenantQueryKey::from_parts(tenant_id, query_name);
        let metrics = self.tenant_metrics.get(&key)?;
        if metrics.is_empty() {
            return None;
        }

        Some(Self::compute_stats(query_name, metrics))
    }

    fn build_variance_analysis(
        &self,
        query_name: &str,
        metrics: &[QueryMetrics],
    ) -> Option<PerformanceVarianceAnalysis> {
        if metrics.len() < 2 {
            return None;
        }

        let times: Vec<f64> = metrics.iter().map(|m| m.execution_time_us as f64).collect();
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        // Edge case: return 0.0 variance for single-element datasets to avoid division by zero
        let variance = if times.len() > 1 {
            times.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / (times.len() - 1) as f64
        } else {
            0.0
        };
        let std_dev = variance.sqrt();
        let coefficient_of_variation = if mean > 0.0 { std_dev / mean } else { 0.0 };

        let critical_value = 1.96;
        let standard_error = std_dev / (times.len() as f64).sqrt();
        let margin_of_error = critical_value * standard_error;

        let confidence_interval_lower = mean - margin_of_error;
        let confidence_interval_upper = mean + margin_of_error;

        Some(PerformanceVarianceAnalysis {
            query_name: query_name.to_string(),
            sample_size: times.len(),
            mean_time_us: mean,
            variance_us2: variance,
            std_dev_us: std_dev,
            coefficient_of_variation,
            confidence_interval_95_lower: confidence_interval_lower,
            confidence_interval_95_upper: confidence_interval_upper,
            has_significant_variance: coefficient_of_variation
                > self.regression_thresholds.max_variance_coefficient,
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

    /// Analyze query plan performance improvement
    pub fn analyze_query_plan_improvement(
        &self,
        tenant_id: Option<&str>,
        before_plan: &str,
        after_plan: &str,
        before_time_us: u64,
        after_time_us: u64,
    ) -> QueryPlanAnalysis {
        let improvement_percentage = if before_time_us > 0 {
            ((before_time_us as f64 - after_time_us as f64) / before_time_us as f64) * 100.0
        } else {
            0.0
        };

        let index_utilization_before =
            before_plan.contains("SEARCH") && before_plan.contains("idx_adapters");
        let index_utilization_after =
            after_plan.contains("SEARCH") && after_plan.contains("idx_adapters");
        let target_met = improvement_percentage >= 50.0;

        QueryPlanAnalysis {
            tenant_id: tenant_id.map(|s| s.to_string()),
            before_plan: before_plan.to_string(),
            after_plan: after_plan.to_string(),
            before_time_us,
            after_time_us,
            improvement_percentage,
            index_utilization_before,
            index_utilization_after,
            target_met,
        }
    }

    /// Calculate optimization impact for a tenant
    pub fn calculate_optimization_impact(
        &self,
        query_name: &str,
        tenant_id: Option<&str>,
        baseline_latency_us: u64,
    ) -> Option<OptimizationImpact> {
        let stats = self.get_stats(query_name)?;
        let current_latency_us = stats.avg_time_us;

        let improvement_pct = if baseline_latency_us > 0 {
            ((baseline_latency_us as f64 - current_latency_us as f64) / baseline_latency_us as f64)
                * 100.0
        } else {
            0.0
        };

        // Simple ROI: improvement * execution count (proxy for frequency)
        let roi_score = improvement_pct * stats.execution_count as f64;

        Some(OptimizationImpact {
            query_name: query_name.to_string(),
            tenant_id: tenant_id.map(|s| s.to_string()),
            baseline_latency_us,
            current_latency_us,
            improvement_pct,
            roi_score,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Check for performance regressions against latency thresholds
    pub fn check_performance_regression(&self, threshold_ms: u64) -> Vec<RegressionAlert> {
        let mut alerts = Vec::new();
        let stats = self.all_metrics();

        for (query_name, stat) in stats {
            // Check P95 latency against threshold
            let p95_ms = stat.p95_time_us / 1000;
            if p95_ms > threshold_ms {
                alerts.push(RegressionAlert {
                    query_name: query_name.clone(),
                    latency_ms: p95_ms,
                    threshold_ms,
                    tenant_id: None, // Will be set by caller for tenant-scoped queries
                    timestamp: chrono::Utc::now().to_rfc3339(),
                });
            }
        }

        alerts
    }

    /// Record concurrency metric
    pub fn record_concurrency_metric(&mut self, metric: ConcurrencyMetric) {
        self.concurrency_metrics
            .entry(metric.query_name.clone())
            .or_default()
            .push(metric);
    }

    /// Get concurrency statistics for a query
    pub fn get_concurrency_stats(&self, query_name: &str) -> Option<QueryStats> {
        let metrics = self.concurrency_metrics.get(query_name)?;
        if metrics.is_empty() {
            return None;
        }

        // Convert ConcurrencyMetric to QueryMetrics for reuse of stats logic
        let query_metrics: Vec<QueryMetrics> = metrics
            .iter()
            .map(|cm| QueryMetrics {
                query_name: cm.query_name.clone(),
                execution_time_us: cm.execution_time_us,
                rows_returned: None,
                used_index: cm.used_index,
                query_plan: None,
                timestamp: cm.timestamp.clone(),
                tenant_id: None,
            })
            .collect();

        if query_metrics.is_empty() {
            return None;
        }

        Some(Self::compute_stats(query_name, &query_metrics))
    }

    /// Analyze statistical significance of performance changes
    pub fn analyze_performance_variance(
        &self,
        query_name: &str,
    ) -> Option<PerformanceVarianceAnalysis> {
        let metrics = self.metrics.get(query_name)?;
        self.build_variance_analysis(query_name, metrics)
    }

    /// Check for performance regressions against configured thresholds
    pub fn check_threshold_violations(&self) -> Vec<ThresholdViolation> {
        let mut violations = Vec::new();

        for (query_name, metrics) in &self.metrics {
            if metrics.is_empty() {
                continue;
            }
            let stats = Self::compute_stats(query_name, metrics);
            self.evaluate_thresholds_for(query_name, None, &stats, Some(metrics), &mut violations);
        }

        for (key, metrics) in &self.tenant_metrics {
            if metrics.is_empty() {
                continue;
            }
            let stats = Self::compute_stats(&key.query_name, metrics);
            self.evaluate_thresholds_for(
                &key.query_name,
                key.tenant_label(),
                &stats,
                Some(metrics),
                &mut violations,
            );
        }

        violations
    }

    fn evaluate_thresholds_for(
        &self,
        query_name: &str,
        tenant_id: Option<&str>,
        stats: &QueryStats,
        metrics: Option<&[QueryMetrics]>,
        violations: &mut Vec<ThresholdViolation>,
    ) {
        let p95_ms = stats.p95_time_us / 1000;
        if p95_ms > self.regression_thresholds.max_p95_latency_ms {
            let severity = if p95_ms > self.regression_thresholds.max_p95_latency_ms * 2 {
                ViolationSeverity::Critical
            } else {
                ViolationSeverity::Warning
            };

            Self::push_threshold_violation(
                violations,
                query_name,
                tenant_id,
                ViolationType::P95LatencyExceeded {
                    actual_ms: p95_ms,
                    threshold_ms: self.regression_thresholds.max_p95_latency_ms,
                },
                severity,
            );
        }

        let p99_ms = stats.p99_time_us / 1000;
        if p99_ms > self.regression_thresholds.max_p99_latency_ms {
            Self::push_threshold_violation(
                violations,
                query_name,
                tenant_id,
                ViolationType::P99LatencyExceeded {
                    actual_ms: p99_ms,
                    threshold_ms: self.regression_thresholds.max_p99_latency_ms,
                },
                ViolationSeverity::Critical,
            );
        }

        if stats.index_usage_pct < self.regression_thresholds.min_index_usage_pct {
            Self::push_threshold_violation(
                violations,
                query_name,
                tenant_id,
                ViolationType::LowIndexUsage {
                    actual_pct: stats.index_usage_pct,
                    threshold_pct: self.regression_thresholds.min_index_usage_pct,
                },
                ViolationSeverity::Warning,
            );
        }

        if let Some(metrics_slice) = metrics {
            if let Some(variance_analysis) = self.build_variance_analysis(query_name, metrics_slice)
            {
                if variance_analysis.has_significant_variance {
                    Self::push_threshold_violation(
                        violations,
                        query_name,
                        tenant_id,
                        ViolationType::HighVariance {
                            coefficient: variance_analysis.coefficient_of_variation,
                            threshold: self.regression_thresholds.max_variance_coefficient,
                        },
                        ViolationSeverity::Warning,
                    );
                }
            }
        }
    }

    fn push_threshold_violation(
        violations: &mut Vec<ThresholdViolation>,
        query_name: &str,
        tenant_id: Option<&str>,
        violation_type: ViolationType,
        severity: ViolationSeverity,
    ) {
        violations.push(ThresholdViolation {
            query_name: query_name.to_string(),
            violation_type,
            severity,
            tenant_id: tenant_id.map(|t| t.to_string()),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// Capture the current stats snapshot as the optimization baseline
    pub fn capture_baseline(&mut self, tenant_id: Option<&str>, query_name: &str) -> bool {
        if let Some(stats) = self.get_tenant_stats(tenant_id, query_name) {
            let key = TenantQueryKey::from_parts(tenant_id, query_name);
            self.baseline_stats.insert(key, stats);
            true
        } else {
            false
        }
    }

    /// Compute the optimization impact relative to baseline for a tenant and query
    pub fn optimization_impact_for(
        &self,
        tenant_id: Option<&str>,
        query_name: &str,
    ) -> Option<OptimizationImpact> {
        let key = TenantQueryKey::from_parts(tenant_id, query_name);
        let baseline = self.baseline_stats.get(&key)?;
        let current = self.get_tenant_stats(tenant_id, query_name)?;
        Some(OptimizationImpact::from_stats(
            &key,
            baseline,
            &current,
            self.regression_thresholds.min_improvement_pct,
        ))
    }

    /// Enumerate optimization impacts for all stored baselines
    pub fn optimization_impacts_since_baseline(&self) -> Vec<OptimizationImpact> {
        self.baseline_stats
            .iter()
            .filter_map(|(key, baseline)| {
                self.get_tenant_stats(key.tenant_label(), &key.query_name)
                    .map(|current| {
                        OptimizationImpact::from_stats(
                            key,
                            baseline,
                            &current,
                            self.regression_thresholds.min_improvement_pct,
                        )
                    })
            })
            .collect()
    }

    /// Generate performance report with SLA validation
    pub fn report_with_sla_validation(&self, _target_improvement_pct: f64) -> String {
        let mut report = String::from("=== Query Performance Report with SLA Validation ===\n\n");

        let stats = self.all_metrics();
        if stats.is_empty() {
            report.push_str("No queries recorded yet.\n");
            return report;
        }

        let mut total_queries = 0;
        let mut passed_sla = 0;

        for (query_name, stat) in &stats {
            total_queries += 1;
            let avg_ms = stat.avg_time_us as f64 / 1000.0;

            // SLA: Average query time < 50ms for tenant-scoped queries
            let meets_sla = avg_ms < 50.0;
            if meets_sla {
                passed_sla += 1;
            }

            report.push_str(&format!(
                "Query: {} {}\n",
                query_name,
                if meets_sla { "✅" } else { "❌" }
            ));
            report.push_str(&format!("  Executions: {}\n", stat.execution_count));
            report.push_str(&format!("  Time (avg): {:.2}ms (SLA: <50ms)\n", avg_ms));
            report.push_str(&format!("  Index Usage: {:.1}%\n", stat.index_usage_pct));
            report.push('\n');
        }

        report.push_str(&format!(
            "SLA Summary: {}/{} queries meet <50ms requirement\n",
            passed_sla, total_queries
        ));

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

/// Validate performance improvement against 50% target
pub fn validate_performance_target(analysis: &QueryPlanAnalysis) -> ValidationResult {
    if analysis.improvement_percentage >= 50.0 {
        ValidationResult::Passed {
            improvement_pct: analysis.improvement_percentage,
        }
    } else {
        ValidationResult::Failed {
            actual_improvement: analysis.improvement_percentage,
            required_improvement: 50.0,
        }
    }
}

/// Generate regression alerts for performance violations
pub fn generate_regression_alerts(alerts: &[RegressionAlert]) -> Vec<String> {
    alerts
        .iter()
        .map(|alert| {
            format!(
                "🚨 PERFORMANCE REGRESSION: {} exceeded {}ms threshold (actual: {}ms) at {}",
                alert.query_name, alert.threshold_ms, alert.latency_ms, alert.timestamp
            )
        })
        .collect()
}

/// Generate realistic multi-tenant dataset for performance testing
pub async fn generate_multi_tenant_dataset(
    db: &Db,
    config: &MultiTenantConfig,
) -> Result<MultiTenantDataset> {
    use chrono::{Duration, Utc};
    use rand::prelude::*;

    let mut rng = rand::thread_rng();
    let mut tenant_ids = Vec::new();
    let mut total_adapters_created = 0;

    // Create tenants
    for tenant_num in 0..config.tenant_count {
        let tenant_id = format!("perf-test-tenant-{:03}", tenant_num);

        sqlx::query("INSERT INTO tenants (id, name) VALUES (?, ?)")
            .bind(&tenant_id)
            .bind(format!("Performance Test Tenant {}", tenant_num))
            .execute(db.pool())
            .await?;

        tenant_ids.push(tenant_id);
    }

    // Generate adapters for each tenant
    for tenant_id in &tenant_ids {
        // Calculate adapter count with variance
        let base_count = config.adapters_per_tenant_base as f64;
        let variance = config.adapter_count_variance_pct / 100.0;
        let min_count = (base_count * (1.0 - variance)) as usize;
        let max_count = (base_count * (1.0 + variance)) as usize;
        let adapter_count = rng.gen_range(min_count..=max_count);

        for adapter_num in 0..adapter_count {
            // Determine if adapter should be active
            let is_active = rng.gen_bool(config.active_adapter_ratio);

            // Select tier based on weights
            let tier = select_weighted_tier(&config.tier_weights, &mut rng);

            // Generate random creation date within range
            let days_ago = rng.gen_range(0..config.creation_date_range_days);
            let created_at = Utc::now() - Duration::days(days_ago as i64);

            let adapter_id = format!("perf-adapter-{}-{:04}", tenant_id, adapter_num);
            let name = format!("Performance Test Adapter {}", adapter_num);
            let hash = format!("perf-hash-{:064x}", rng.gen::<u64>());
            let memory_bytes = rng.gen_range(100_u64 * 1024 * 1024..10_u64 * 1024 * 1024 * 1024); // 100MB to 10GB

            sqlx::query(
                r#"
                INSERT INTO adapters (
                    id, tenant_id, name, tier, hash_b3, rank, alpha,
                    targets_json, adapter_id, active, lifecycle_state,
                    load_state, activation_count, memory_bytes, created_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(format!("perf-id-{}-{}", tenant_id, adapter_num))
            .bind(tenant_id)
            .bind(&name)
            .bind(&tier)
            .bind(&hash)
            .bind(16)
            .bind(rng.gen_range(16.0..64.0))
            .bind("[]")
            .bind(&adapter_id)
            .bind(is_active)
            .bind("active")
            .bind("cold")
            .bind(rng.gen_range(0..1000))
            .bind(memory_bytes as i64)
            .bind(created_at.to_rfc3339())
            .execute(db.pool())
            .await?;

            total_adapters_created += 1;
        }
    }

    Ok(MultiTenantDataset {
        tenant_ids,
        total_adapters: total_adapters_created,
        adapters_per_tenant: config.adapters_per_tenant_base,
    })
}

/// Helper function to select tier based on weighted distribution
fn select_weighted_tier(tier_weights: &HashMap<String, f64>, rng: &mut impl Rng) -> String {
    let total_weight: f64 = tier_weights.values().sum();
    let mut cumulative = 0.0;
    let random_value = rng.gen::<f64>() * total_weight;

    for (tier, weight) in tier_weights {
        cumulative += weight;
        if random_value <= cumulative {
            return tier.clone();
        }
    }

    // Fallback to first tier if weights don't sum properly
    tier_weights
        .keys()
        .next()
        .unwrap_or(&"warm".to_string())
        .clone()
}

/// Run concurrent performance test to simulate high-concurrency scenarios
pub async fn run_concurrency_performance_test(
    db: &Db,
    config: &ConcurrencyConfig,
    tenant_ids: &[String],
) -> Result<Vec<ConcurrencyMetric>> {
    use std::sync::Arc;
    use tokio::task;

    let db: std::sync::Arc<Db> = Arc::new(db.clone());
    let tenant_ids: std::sync::Arc<Vec<String>> = Arc::new(tenant_ids.to_vec());
    let mut handles = Vec::new();
    let mut all_metrics = Vec::new();

    info!(
        concurrent_workers = config.concurrent_workers,
        test_duration_ms = config.test_duration_ms,
        queries_per_second_per_worker = config.queries_per_second_per_worker,
        tenant_count = tenant_ids.len(),
        "Starting concurrency performance test"
    );

    let start_time = Instant::now();

    // Spawn concurrent workers
    for worker_id in 0..config.concurrent_workers {
        let db = Arc::clone(&db);
        let tenant_ids = Arc::clone(&tenant_ids);
        let worker_metrics = Arc::new(std::sync::Mutex::new(Vec::new()));
        let test_duration_ms = config.test_duration_ms;
        let queries_per_second_per_worker = config.queries_per_second_per_worker;

        let handle = task::spawn(async move {
            let mut rng = rand::rngs::StdRng::from_entropy();
            let worker_start = Instant::now();

            while worker_start.elapsed().as_millis() < test_duration_ms as u128 {
                let tenant_id = &tenant_ids[rng.gen_range(0..tenant_ids.len())];
                let query_start = Instant::now();

                // Execute tenant-scoped adapter listing query
                let result = sqlx::query(
                    "SELECT * FROM adapters WHERE tenant_id = ? AND active = 1 ORDER BY tier ASC, created_at DESC"
                )
                .bind(tenant_id)
                .fetch_all(db.pool())
                .await;

                let execution_time = query_start.elapsed();

                match result {
                    Ok(_rows) => {
                        let metric = ConcurrencyMetric {
                            query_name: "concurrent_tenant_adapter_listing".to_string(),
                            worker_id,
                            execution_time_us: execution_time.as_micros() as u64,
                            used_index: true, // Assume index is used (would be validated in real scenario)
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            memory_usage_bytes: None, // Could be added with memory profiling
                        };

                        worker_metrics.lock().unwrap().push(metric);
                    }
                    Err(e) => {
                        warn!("Query failed in worker {}: {:?}", worker_id, e);
                    }
                }

                // Rate limiting: sleep to achieve target queries per second
                let target_interval_ms = (1000 / queries_per_second_per_worker) as u64;
                let elapsed_since_query = query_start.elapsed().as_millis() as u64;
                if elapsed_since_query < target_interval_ms {
                    tokio::time::sleep(tokio::time::Duration::from_millis(
                        target_interval_ms - elapsed_since_query,
                    ))
                    .await;
                }
            }

            // Return metrics from this worker
            let metrics = worker_metrics.lock().unwrap().clone();
            metrics
        });

        handles.push(handle);
    }

    // Collect results from all workers
    for handle in handles {
        match handle.await {
            Ok(metrics) => {
                all_metrics.extend(metrics);
            }
            Err(e) => {
                warn!("Worker task failed: {:?}", e);
            }
        }
    }

    let total_duration = start_time.elapsed();
    info!(
        total_duration_secs = total_duration.as_secs_f64(),
        total_queries = all_metrics.len(),
        "Concurrency test completed"
    );

    if !all_metrics.is_empty() {
        let avg_latency = all_metrics.iter().map(|m| m.execution_time_us).sum::<u64>() as f64
            / all_metrics.len() as f64;

        let p95_latency = {
            let mut times: Vec<u64> = all_metrics.iter().map(|m| m.execution_time_us).collect();
            times.sort_unstable();
            let p95_idx = (times.len() * 95) / 100;
            times.get(p95_idx).copied().unwrap_or(0)
        };

        info!(
            avg_latency_ms = avg_latency / 1000.0,
            p95_latency_ms = p95_latency as f64 / 1000.0,
            queries_per_second = all_metrics.len() as f64 / total_duration.as_secs_f64(),
            "Concurrent query statistics"
        );
    }

    Ok(all_metrics)
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
                tenant_id: None,
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
        // High variance: variance = max - min = 200000 - 1000 = 199000
        // threshold = avg * 10 = 5000 * 10 = 50000
        // 199000 > 50000 triggers "High variance"
        // index_usage_pct < 50.0 triggers "Consider adding indexes"
        let recommendations = generate_recommendations("slow_query", 1000, 200000, 5000, 10.0);
        assert!(!recommendations.is_empty());
        assert!(recommendations.iter().any(|r| r.contains("High variance")));
        assert!(recommendations
            .iter()
            .any(|r| r.contains("Consider adding indexes")));
    }
}
