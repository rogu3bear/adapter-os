//! Automated Index Analysis Job for Tenant-Scoped Operations
//!
//! This module provides scheduled maintenance tasks that run ANALYZE on all tenant-scoped
//! tables and indexes, ensuring SQLite's query planner always has current statistics for
//! optimal execution plan selection. This prevents performance degradation over time as
//! data distributions change and maintains the performance benefits of migration 0210's
//! indexes.

use crate::{
    Db, IndexAnalysisJobConfig, IndexAnalysisJobStatus, IndexAnalysisResult, 
    IndexAnalysisStats, QueryPerformanceMonitor, TenantIndexCoverage
};
use adapteros_core::{AosError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Configuration for the automated index analysis job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexAnalysisJobConfig {
    /// Enable/disable automated index analysis
    pub enabled: bool,
    /// How often to run analysis (in minutes)
    pub interval_minutes: u32,
    /// Tenant-scoped tables to analyze (empty = auto-detect)
    pub tenant_scoped_tables: Vec<String>,
    /// Performance threshold for triggering analysis (ms)
    pub performance_threshold_ms: u64,
    /// Enable aggressive analysis for performance regression
    pub aggressive_mode: bool,
    /// Tables that should always be analyzed
    pub critical_tables: Vec<String>,
}

/// Status tracking for the index analysis job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexAnalysisJobStatus {
    /// Whether the job is currently running
    pub is_running: bool,
    /// Last analysis timestamp
    pub last_run: Option<DateTime<Utc>>,
    /// Next scheduled run
    pub next_run: Option<DateTime<Utc>>,
    /// Number of analyses performed
    pub total_runs: u64,
    /// Success/failure tracking
    pub successful_runs: u64,
    pub failed_runs: u64,
    /// Last error message if any
    pub last_error: Option<String>,
    /// Current performance baseline
    pub performance_baseline_ms: u64,
}

/// Results from an index analysis run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexAnalysisResult {
    /// Analysis timestamp
    pub timestamp: DateTime<Utc>,
    /// Tables analyzed
    pub tables_analyzed: Vec<String>,
    /// Total execution time
    pub execution_time_ms: u64,
    /// Performance before analysis
    pub performance_before: HashMap<String, u64>,
    /// Performance after analysis
    pub performance_after: HashMap<String, u64>,
    /// Index coverage check results
    pub index_coverage: Vec<TenantIndexCoverage>,
    /// Analysis statistics
    pub stats: IndexAnalysisStats,
}

/// Statistics from an index analysis run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexAnalysisStats {
    /// Number of tables analyzed
    pub table_count: usize,
    /// Number of indexes optimized
    pub index_count: usize,
    /// Performance improvement percentage
    pub improvement_percentage: f64,
    /// Whether aggressive mode was used
    pub aggressive_mode_used: bool,
    /// Tables that showed performance regression
    pub regressed_tables: Vec<String>,
    /// Tables that showed performance improvement
    pub improved_tables: Vec<String>,
}

/// The automated index analysis job
pub struct IndexAnalysisJob {
    /// Database connection
    db: Db,
    /// Job configuration
    config: IndexAnalysisJobConfig,
    /// Job status
    status: IndexAnalysisJobStatus,
    /// Performance monitor for tracking effectiveness
    performance_monitor: Option<QueryPerformanceMonitor>,
    /// Auto-detected tenant-scoped tables
    detected_tenant_tables: Vec<String>,
}

impl IndexAnalysisJob {
    /// Create a new index analysis job
    pub fn new(db: Db, config: IndexAnalysisJobConfig) -> Self {
        Self {
            db,
            config,
            status: IndexAnalysisJobStatus {
                is_running: false,
                last_run: None,
                next_run: None,
                total_runs: 0,
                successful_runs: 0,
                failed_runs: 0,
                last_error: None,
                performance_baseline_ms: 100, // Default 100ms baseline
            },
            performance_monitor: None,
            detected_tenant_tables: Vec::new(),
        }
    }

    /// Create with performance monitor integration
    pub fn with_performance_monitor(
        db: Db, 
        config: IndexAnalysisJobConfig, 
        performance_monitor: QueryPerformanceMonitor
    ) -> Self {
        Self {
            db,
            config,
            status: IndexAnalysisJobStatus {
                is_running: false,
                last_run: None,
                next_run: None,
                total_runs: 0,
                successful_runs: 0,
                failed_runs: 0,
                last_error: None,
                performance_baseline_ms: 100,
            },
            performance_monitor: Some(performance_monitor),
            detected_tenant_tables: Vec::new(),
        }
    }

    /// Get the current job status
    pub fn status(&self) -> &IndexAnalysisJobStatus {
        &self.status
    }

    /// Update the job configuration
    pub fn update_config(&mut self, config: IndexAnalysisJobConfig) {
        self.config = config;
        info!("Index analysis job configuration updated");
    }

    /// Run a single analysis cycle
    pub async fn run_analysis(&mut self) -> Result<IndexAnalysisResult> {
        use std::time::Instant;
        use tracing::info;

        if self.status.is_running {
            warn!("Index analysis job is already running, skipping this cycle");
            return Err(AosError::Database("Analysis job already running".to_string()).into());
        }

        self.status.is_running = true;
        self.status.total_runs += 1;

        let start_time = Instant::now();
        info!("Starting automated index analysis job");

        // Step 1: Auto-detect tenant-scoped tables if not configured
        if self.config.tenant_scoped_tables.is_empty() {
            self.detect_tenant_scoped_tables().await?;
        }

        // Step 2: Determine which tables to analyze
        let tables_to_analyze = self.get_tables_to_analyze();

        // Step 3: Collect performance baseline
        let performance_before = self.collect_performance_baseline().await?;

        // Step 4: Run ANALYZE on all relevant tables
        info!("Running ANALYZE on {} tables", tables_to_analyze.len());
        for table in &tables_to_analyze {
            self.db.sqlite_analyze_tables(&[table]).await.map_err(|e| {
                warn!("Failed to analyze table {}: {}", table, e);
                e
            })?;
        }

        // Step 5: Check index coverage
        let index_coverage = self.db.collect_tenant_index_coverage(&tables_to_analyze).await?;

        // Step 6: Collect performance after analysis
        let performance_after = self.collect_performance_baseline().await?;

        // Step 7: Calculate statistics
        let stats = self.calculate_analysis_stats(&performance_before, &performance_after);

        let execution_time = start_time.elapsed();
        let execution_time_ms = execution_time.as_millis() as u64;

        // Step 8: Update status
        let now = Utc::now();
        self.status.last_run = Some(now);
        self.status.next_run = Some(now + Duration::from_secs(self.config.interval_minutes as u64 * 60));
        self.status.successful_runs += 1;
        self.status.last_error = None;
        self.status.performance_baseline_ms = self.calculate_new_baseline(&performance_after);

        info!(
            execution_time_ms = execution_time_ms,
            table_count = tables_to_analyze.len(),
            improvement_percentage = stats.improvement_percentage,
            "Automated index analysis job completed successfully"
        );

        self.status.is_running = false;

        Ok(IndexAnalysisResult {
            timestamp: now,
            tables_analyzed: tables_to_analyze,
            execution_time_ms,
            performance_before,
            performance_after,
            index_coverage,
            stats,
        })
    }

    /// Auto-detect tenant-scoped tables from the database schema
    async fn detect_tenant_scoped_tables(&mut self) -> Result<()> {
        let Some(pool) = self.db.pool_opt() else {
            return Ok(());
        };

        // Query sqlite_master for all tables
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to query tables: {}", e)))?;

        let mut detected_tables = Vec::new();

        for table in tables {
            // Check if table has tenant_id column
            let table_info_rows = sqlx::query(&format!("PRAGMA table_info(\"{}\")", table))
                .fetch_all(pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to get table info for {}: {}", table, e)))?;

            let has_tenant_id = table_info_rows
                .iter()
                .any(|row| row.get::<String, _>("name") == "tenant_id");

            if has_tenant_id {
                detected_tables.push(table);
                debug!("Detected tenant-scoped table: {}", table);
            }
        }

        self.detected_tenant_tables = detected_tables;
        info!("Auto-detected {} tenant-scoped tables", self.detected_tenant_tables.len());

        Ok(())
    }

    /// Get the list of tables to analyze based on configuration and detection
    fn get_tables_to_analyze(&self) -> Vec<String> {
        let mut tables = Vec::new();

        // Add explicitly configured tables
        tables.extend(self.config.tenant_scoped_tables.clone());

        // Add auto-detected tables (if not already included)
        for table in &self.detected_tenant_tables {
            if !tables.contains(table) {
                tables.push(table.clone());
            }
        }

        // Always include critical tables
        for table in &self.config.critical_tables {
            if !tables.contains(table) {
                tables.push(table.clone());
            }
        }

        tables.sort();
        tables
    }

    /// Collect performance baseline from query performance monitor
    async fn collect_performance_baseline(&self) -> Result<HashMap<String, u64>> {
        let mut baseline = HashMap::new();

        if let Some(monitor) = &self.performance_monitor {
            let all_stats = monitor.all_metrics();
            
            for (query_name, stats) in all_stats {
                // Use P95 latency as the baseline metric
                baseline.insert(query_name, stats.p95_time_us / 1000); // Convert to ms
            }
        }

        Ok(baseline)
    }

    /// Calculate analysis statistics
    fn calculate_analysis_stats(
        &self,
        before: &HashMap<String, u64>,
        after: &HashMap<String, u64>
    ) -> IndexAnalysisStats {
        let mut total_improvement = 0.0;
        let mut improved_count = 0;
        let mut regressed_count = 0;
        let mut improved_tables = Vec::new();
        let mut regressed_tables = Vec::new();

        // Compare performance for each query
        for (query_name, before_latency) in before {
            if let Some(&after_latency) = after.get(query_name) {
                let improvement = if *before_latency > 0 {
                    ((*before_latency as f64 - after_latency as f64) / *before_latency as f64) * 100.0
                } else {
                    0.0
                };

                if improvement > 5.0 { // 5% improvement threshold
                    improved_count += 1;
                    total_improvement += improvement;
                    improved_tables.push(query_name.clone());
                } else if improvement < -5.0 { // 5% regression threshold
                    regressed_count += 1;
                    regressed_tables.push(query_name.clone());
                }
            }
        }

        let avg_improvement = if improved_count > 0 {
            total_improvement / improved_count as f64
        } else {
            0.0
        };

        IndexAnalysisStats {
            table_count: after.len(),
            index_count: 0, // Will be populated when we add index counting
            improvement_percentage: avg_improvement,
            aggressive_mode_used: self.config.aggressive_mode,
            regressed_tables,
            improved_tables,
        }
    }

    /// Calculate new performance baseline from current metrics
    fn calculate_new_baseline(&self, performance: &HashMap<String, u64>) -> u64 {
        if performance.is_empty() {
            return self.status.performance_baseline_ms;
        }

        // Use median P95 latency as the new baseline
        let mut latencies: Vec<u64> = performance.values().cloned().collect();
        latencies.sort();
        
        if latencies.is_empty() {
            self.status.performance_baseline_ms
        } else if latencies.len() % 2 == 0 {
            let mid = latencies.len() / 2;
            (latencies[mid - 1] + latencies[mid]) / 2
        } else {
            latencies[latencies.len() / 2]
        }
    }

    /// Check if analysis should be triggered based on performance regression
    pub async fn should_trigger_analysis(&self) -> Result<bool> {
        if !self.config.enabled || self.status.is_running {
            return Ok(false);
        }

        // Check if it's time for scheduled analysis
        if let Some(next_run) = self.status.next_run {
            if Utc::now() >= next_run {
                return Ok(true);
            }
        }

        // Check performance regression trigger
        if let Some(monitor) = &self.performance_monitor {
            let all_stats = monitor.all_metrics();
            
            for (_query_name, stats) in all_stats {
                let p95_ms = stats.p95_time_us / 1000;
                if p95_ms > self.status.performance_baseline_ms + self.config.performance_threshold_ms {
                    info!(
                        query = _query_name,
                        p95_ms = p95_ms,
                        baseline_ms = self.status.performance_baseline_ms,
                        threshold_ms = self.config.performance_threshold_ms,
                        "Performance regression detected, triggering index analysis"
                    );
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Run aggressive analysis (used when performance regression is detected)
    pub async fn run_aggressive_analysis(&mut self) -> Result<IndexAnalysisResult> {
        info!("Running aggressive index analysis due to performance regression");
        
        // Temporarily enable aggressive mode
        let original_aggressive = self.config.aggressive_mode;
        self.config.aggressive_mode = true;

        let result = self.run_analysis().await;

        // Restore original configuration
        self.config.aggressive_mode = original_aggressive;

        result
    }

    /// Get job statistics for monitoring
    pub fn get_statistics(&self) -> HashMap<String, serde_json::Value> {
        let mut stats = HashMap::new();
        
        stats.insert("is_running".to_string(), serde_json::Value::Bool(self.status.is_running));
        stats.insert("total_runs".to_string(), serde_json::Value::Number(serde_json::Number::from(self.status.total_runs)));
        stats.insert("success_rate".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(if self.status.total_runs > 0 {
            self.status.successful_runs as f64 / self.status.total_runs as f64 * 100.0
        } else {
            0.0
        }).unwrap_or(0.0)));
        stats.insert("performance_baseline_ms".to_string(), serde_json::Value::Number(serde_json::Number::from(self.status.performance_baseline_ms)));
        stats.insert("detected_tables".to_string(), serde_json::Value::Number(serde_json::Number::from(self.detected_tenant_tables.len())));
        
        if let Some(last_run) = self.status.last_run {
            stats.insert("last_run".to_string(), serde_json::Value::String(last_run.to_rfc3339()));
        }
        
        if let Some(next_run) = self.status.next_run {
            stats.insert("next_run".to_string(), serde_json::Value::String(next_run.to_rfc3339()));
        }

        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;

    #[tokio::test]
    async fn test_index_analysis_job_creation() {
        let db = Db::new_in_memory().await.unwrap();
        let config = IndexAnalysisJobConfig {
            enabled: true,
            interval_minutes: 60,
            tenant_scoped_tables: vec!["adapters".to_string()],
            performance_threshold_ms: 50,
            aggressive_mode: false,
            critical_tables: vec!["adapters".to_string(), "users".to_string()],
        };

        let job = IndexAnalysisJob::new(db, config);
        assert!(job.status().is_running == false);
        assert_eq!(job.status().total_runs, 0);
    }

    #[tokio::test]
    async fn test_auto_detection_of_tenant_tables() {
        let db = Db::new_in_memory().await.unwrap();
        
        // Create some test tables with tenant_id
        sqlx::query("CREATE TABLE test_tenants (id TEXT PRIMARY KEY, tenant_id TEXT, data TEXT)")
            .execute(db.pool_result()?)
            .await
            .unwrap();
        
        sqlx::query("CREATE TABLE test_no_tenant (id TEXT PRIMARY KEY, data TEXT)")
            .execute(db.pool_result()?)
            .await
            .unwrap();

        let config = IndexAnalysisJobConfig {
            enabled: true,
            interval_minutes: 60,
            tenant_scoped_tables: vec![],
            performance_threshold_ms: 50,
            aggressive_mode: false,
            critical_tables: vec![],
        };

        let mut job = IndexAnalysisJob::new(db, config);
        job.detect_tenant_scoped_tables().await.unwrap();
        
        assert!(job.detected_tenant_tables.contains(&"test_tenants".to_string()));
        assert!(!job.detected_tenant_tables.contains(&"test_no_tenant".to_string()));
    }
}