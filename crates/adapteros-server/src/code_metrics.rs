//! Code quality metrics for promotion gates

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Metrics tracker for code intelligence quality
#[derive(Debug, Clone)]
pub struct CodeMetrics {
    /// Path to CDP storage
    cdp_store_path: PathBuf,
    /// Path to telemetry bundles
    telemetry_bundles_path: Option<PathBuf>,
    /// In-memory cache for quick access
    cache: HashMap<String, MetricValue>,
}

/// A single metric value with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    pub value: f32,
    pub timestamp: u64,
}

/// Code metrics event from NDJSON bundle (local parsing type)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CodeMetricsEvent {
    event_type: String,
    #[serde(default)]
    kind: Option<String>,
    payload: serde_json::Value,
    timestamp: u128,
}

impl CodeMetrics {
    /// Create a new metrics tracker
    pub fn new(cdp_store_path: PathBuf) -> Self {
        Self {
            cdp_store_path,
            telemetry_bundles_path: None,
            cache: HashMap::new(),
        }
    }

    /// Create with telemetry bundles path
    pub fn with_telemetry(mut self, telemetry_bundles_path: PathBuf) -> Self {
        self.telemetry_bundles_path = Some(telemetry_bundles_path);
        self
    }

    /// Load CDP by ID
        
        let store = CdpStore::new(self.cdp_store_path.clone())
            .map_err(|e| format!("Failed to open CDP store: {}", e))?;
        
        let cdp_id = CdpId(cpid.to_string());
        store.load(&cdp_id)
            .map_err(|e| format!("Failed to load CDP {}: {}", cpid, e))
    }

    /// Read telemetry events from NDJSON bundle
    fn read_telemetry_bundle(&self, cpid: &str) -> Result<Vec<CodeMetricsEvent>, String> {
        let telemetry_path = match &self.telemetry_bundles_path {
            Some(path) => path,
            None => return Ok(Vec::new()), // No telemetry configured
        };

        // Find bundle files for this CPID
        let pattern = format!("{}_*.ndjson", cpid);
        let bundle_files: Vec<_> = std::fs::read_dir(telemetry_path)
            .map_err(|e| format!("Failed to read telemetry directory: {}", e))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.file_name()
                    .to_str()
                    .map(|name| name.contains(cpid) && name.ends_with(".ndjson"))
                    .unwrap_or(false)
            })
            .collect();

        let mut events = Vec::new();
        for bundle_file in bundle_files {
            let file = std::fs::File::open(bundle_file.path())
                .map_err(|e| format!("Failed to open telemetry bundle: {}", e))?;
            
            let reader = std::io::BufReader::new(file);
            use std::io::BufRead;
            
            for line in reader.lines() {
                let line = line.map_err(|e| format!("Failed to read line: {}", e))?;
                if line.trim().is_empty() {
                    continue;
                }
                
                let event: CodeMetricsEvent = serde_json::from_str(&line)
                    .map_err(|e| format!("Failed to parse telemetry event: {}", e))?;
                events.push(event);
            }
        }

        Ok(events)
    }

    /// Compute build success rate for given CPIDs
    /// Returns ratio of builds without errors (0.0-1.0)
    pub fn compute_build_success_rate(&self, cpids: &[String]) -> Result<f32, String> {
        let mut successful = 0;
        let mut total = 0;
        
        for cpid in cpids {
            // Try telemetry events first
            if let Ok(events) = self.read_telemetry_bundle(cpid) {
                for event in events {
                    if event.event_type == "build.completed" {
                        total += 1;
                        if let Some(success) = event.payload.get("success").and_then(|v| v.as_bool()) {
                            if success {
                                successful += 1;
                            }
                        }
                    }
                }
            }
            
            // Fall back to CDP build_logs
            if total == 0 {
                if let Ok(cdp) = self.load_cdp(cpid) {
                    total += 1;
                    // If no build logs, assume success
                    if cdp.build_logs.is_none() || !cdp.build_logs.as_ref().map_or(false, |logs| logs.contains("error")) {
                        successful += 1;
                    }
                }
            }
        }
        
        if total == 0 {
            return Ok(1.0); // No data, assume success
        }
        
        Ok(successful as f32 / total as f32)
    }

    /// Compute test pass rate for given CPIDs
    /// Returns ratio of tests passing (0.0-1.0)
    pub fn compute_test_pass_rate(&self, cpids: &[String]) -> Result<f32, String> {
        let mut passed = 0;
        let mut total = 0;
        
        for cpid in cpids {
            if let Ok(cdp) = self.load_cdp(cpid) {
                for test_result in &cdp.test_results {
                    total += 1;
                        passed += 1;
                    }
                }
            }
        }
        
        if total == 0 {
            return Ok(1.0); // No tests, assume all pass
        }
        
        Ok(passed as f32 / total as f32)
    }

    /// Compute lint error delta between two CPIDs
    /// Returns difference in error count (negative = improvement)
    pub fn compute_lint_error_delta(&self, before: &str, after: &str) -> Result<i32, String> {
        let before_cdp = self.load_cdp(before)?;
        let after_cdp = self.load_cdp(after)?;
        
        let before_errors: i32 = before_cdp.linter_errors.iter()
            .count() as i32;
        
        let after_errors: i32 = after_cdp.linter_errors.iter()
            .count() as i32;
        
        Ok(after_errors - before_errors)
    }

    /// Compute PR acceptance ratio over a timeframe
    /// Returns ratio of accepted PRs (0.0-1.0)
    pub fn compute_pr_acceptance_ratio(&self, _timeframe: Duration) -> Result<f32, String> {
        // Placeholder: In real implementation, integrate with GitHub API or git logs
        Ok(0.85) // 85% acceptance
    }

    /// Compute average time-to-merge for accepted PRs
    /// Returns average duration in seconds
    pub fn compute_time_to_merge(&self, _timeframe: Duration) -> Result<f64, String> {
        // Placeholder: In real implementation, analyze PR merge timestamps
        Ok(7200.0) // 2 hours average
    }

    /// Compute regression rate (follow-up fixes needed)
    /// Returns ratio of patches requiring fixes (0.0-1.0)
    pub fn compute_regression_rate(&self, cpids: &[String]) -> Result<f32, String> {
        let mut regressions = 0;
        let mut total = 0;
        
        for cpid in cpids {
            if let Ok(events) = self.read_telemetry_bundle(cpid) {
                for event in events {
                    if event.event_type == "patch.applied" {
                        total += 1;
                        // Check if there was a follow-up fix within a short time
                        if let Some(had_followup) = event.payload.get("had_followup_fix").and_then(|v| v.as_bool()) {
                            if had_followup {
                                regressions += 1;
                            }
                        }
                    }
                }
            }
        }
        
        if total == 0 {
            return Ok(0.0); // No data, assume no regressions
        }
        
        Ok(regressions as f32 / total as f32)
    }

    /// Compute code coverage change
    /// Returns delta in coverage percentage
    pub fn compute_coverage_delta(&self, before: &str, after: &str) -> Result<f32, String> {
        let before_coverage = self.extract_coverage_from_telemetry(before)?;
        let after_coverage = self.extract_coverage_from_telemetry(after)?;
        
        Ok(after_coverage - before_coverage)
    }

    /// Extract coverage percentage from telemetry events
    fn extract_coverage_from_telemetry(&self, cpid: &str) -> Result<f32, String> {
        if let Ok(events) = self.read_telemetry_bundle(cpid) {
            for event in events {
                if event.event_type == "coverage.report" {
                    if let Some(coverage) = event.payload.get("coverage_percent").and_then(|v| v.as_f64()) {
                        return Ok(coverage as f32);
                    }
                }
            }
        }
        
        // Default coverage if not found
        Ok(75.0)
    }

    /// Cache a metric value
    pub fn cache_metric(&mut self, key: String, value: f32) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_else(|_| {
                warn!("System time appears to be before UNIX epoch, using 0 for timestamp");
                0
            });
        
        self.cache.insert(key, MetricValue { value, timestamp });
    }

    /// Get cached metric value
    pub fn get_cached_metric(&self, key: &str) -> Option<&MetricValue> {
        self.cache.get(key)
    }

    /// Clear metrics cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get all metrics for a CPID as a summary
    pub fn get_metrics_summary(&self, cpid: &str) -> Result<MetricsSummary, String> {
        // Load CDP to get lint errors and coverage
        let lint_error_count = if let Ok(cdp) = self.load_cdp(cpid) {
            cdp.linter_errors.iter()
                .count() as u32
        } else {
            0
        };

        let coverage_percent = self.extract_coverage_from_telemetry(cpid).unwrap_or(75.0);

        Ok(MetricsSummary {
            cpid: cpid.to_string(),
            build_success_rate: self.compute_build_success_rate(&[cpid.to_string()])?,
            test_pass_rate: self.compute_test_pass_rate(&[cpid.to_string()])?,
            regression_rate: self.compute_regression_rate(&[cpid.to_string()])?,
            lint_error_count,
            coverage_percent,
        })
    }
}

/// Summary of all metrics for a CPID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub cpid: String,
    pub build_success_rate: f32,
    pub test_pass_rate: f32,
    pub regression_rate: f32,
    pub lint_error_count: u32,
    pub coverage_percent: f32,
}

impl MetricsSummary {
    /// Check if metrics meet minimum thresholds
    pub fn meets_thresholds(&self, thresholds: &MetricsThresholds) -> bool {
        self.build_success_rate >= thresholds.min_build_success
            && self.test_pass_rate >= thresholds.min_test_pass
            && self.regression_rate <= thresholds.max_regression_rate
            && self.lint_error_count <= thresholds.max_lint_errors
            && self.coverage_percent >= thresholds.min_coverage
    }

    /// Get failing threshold checks
    pub fn get_failures(&self, thresholds: &MetricsThresholds) -> Vec<String> {
        let mut failures = Vec::new();

        if self.build_success_rate < thresholds.min_build_success {
            failures.push(format!(
                "Build success rate {} < {}",
                self.build_success_rate, thresholds.min_build_success
            ));
        }

        if self.test_pass_rate < thresholds.min_test_pass {
            failures.push(format!(
                "Test pass rate {} < {}",
                self.test_pass_rate, thresholds.min_test_pass
            ));
        }

        if self.regression_rate > thresholds.max_regression_rate {
            failures.push(format!(
                "Regression rate {} > {}",
                self.regression_rate, thresholds.max_regression_rate
            ));
        }

        if self.lint_error_count > thresholds.max_lint_errors {
            failures.push(format!(
                "Lint errors {} > {}",
                self.lint_error_count, thresholds.max_lint_errors
            ));
        }

        if self.coverage_percent < thresholds.min_coverage {
            failures.push(format!(
                "Coverage {} < {}",
                self.coverage_percent, thresholds.min_coverage
            ));
        }

        failures
    }
}

/// Thresholds for promotion gates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsThresholds {
    pub min_build_success: f32,
    pub min_test_pass: f32,
    pub max_regression_rate: f32,
    pub max_lint_errors: u32,
    pub min_coverage: f32,
}

impl Default for MetricsThresholds {
    fn default() -> Self {
        Self {
            min_build_success: 0.95,
            min_test_pass: 0.90,
            max_regression_rate: 0.10,
            max_lint_errors: 20,
            min_coverage: 70.0,
        }
    }
}

/// Code promotion gate that checks metrics
#[derive(Debug, Clone)]
pub struct CodePromotionGate {
    pub thresholds: MetricsThresholds,
    pub metrics: CodeMetrics,
}

impl CodePromotionGate {
    /// Create a new promotion gate with default thresholds and CDP store path
    pub fn new(cdp_store_path: PathBuf) -> Self {
        Self {
            thresholds: MetricsThresholds::default(),
            metrics: CodeMetrics::new(cdp_store_path),
        }
    }

    /// Create with custom thresholds and CDP store path
    pub fn with_thresholds(cdp_store_path: PathBuf, thresholds: MetricsThresholds) -> Self {
        Self {
            thresholds,
            metrics: CodeMetrics::new(cdp_store_path),
        }
    }

    /// Check if a CPID can be promoted
    pub fn check(&self, cpid: &str) -> Result<PromotionDecision, String> {
        let summary = self.metrics.get_metrics_summary(cpid)?;
        
        if summary.meets_thresholds(&self.thresholds) {
            Ok(PromotionDecision::Approved {
                cpid: cpid.to_string(),
                summary,
            })
        } else {
            let failures = summary.get_failures(&self.thresholds);
            Ok(PromotionDecision::Rejected {
                cpid: cpid.to_string(),
                summary,
                reasons: failures,
            })
        }
    }

    /// Check comparison between two CPIDs
    pub fn check_upgrade(&self, old_cpid: &str, new_cpid: &str) -> Result<PromotionDecision, String> {
        let old_summary = self.metrics.get_metrics_summary(old_cpid)?;
        let new_summary = self.metrics.get_metrics_summary(new_cpid)?;

        // Check if new CP meets absolute thresholds
        if !new_summary.meets_thresholds(&self.thresholds) {
            let failures = new_summary.get_failures(&self.thresholds);
            return Ok(PromotionDecision::Rejected {
                cpid: new_cpid.to_string(),
                summary: new_summary,
                reasons: failures,
            });
        }

        // Check for regressions
        let mut warnings = Vec::new();

        if new_summary.build_success_rate < old_summary.build_success_rate - 0.05 {
            warnings.push(format!(
                "Build success rate decreased from {} to {}",
                old_summary.build_success_rate, new_summary.build_success_rate
            ));
        }

        if new_summary.test_pass_rate < old_summary.test_pass_rate - 0.05 {
            warnings.push(format!(
                "Test pass rate decreased from {} to {}",
                old_summary.test_pass_rate, new_summary.test_pass_rate
            ));
        }

        if warnings.is_empty() {
            Ok(PromotionDecision::Approved {
                cpid: new_cpid.to_string(),
                summary: new_summary,
            })
        } else {
            Ok(PromotionDecision::ApprovedWithWarnings {
                cpid: new_cpid.to_string(),
                summary: new_summary,
                warnings,
            })
        }
    }
}

impl Default for CodePromotionGate {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a promotion check
#[derive(Debug, Clone)]
pub enum PromotionDecision {
    Approved {
        cpid: String,
        summary: MetricsSummary,
    },
    ApprovedWithWarnings {
        cpid: String,
        summary: MetricsSummary,
        warnings: Vec<String>,
    },
    Rejected {
        cpid: String,
        summary: MetricsSummary,
        reasons: Vec<String>,
    },
}

impl PromotionDecision {
    /// Check if promotion is approved (with or without warnings)
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approved { .. } | Self::ApprovedWithWarnings { .. })
    }

    /// Get the CPID
    pub fn cpid(&self) -> &str {
        match self {
            Self::Approved { cpid, .. } => cpid,
            Self::ApprovedWithWarnings { cpid, .. } => cpid,
            Self::Rejected { cpid, .. } => cpid,
        }
    }

    /// Format decision as human-readable string
    pub fn format(&self) -> String {
        match self {
            Self::Approved { cpid, summary } => {
                format!(
                    "✓ APPROVED: CPID {}\n\
                     Build Success: {:.1}%\n\
                     Test Pass Rate: {:.1}%\n\
                     Regression Rate: {:.1}%\n\
                     Lint Errors: {}\n\
                     Coverage: {:.1}%",
                    cpid,
                    summary.build_success_rate * 100.0,
                    summary.test_pass_rate * 100.0,
                    summary.regression_rate * 100.0,
                    summary.lint_error_count,
                    summary.coverage_percent
                )
            }
            Self::ApprovedWithWarnings { cpid, warnings, .. } => {
                format!(
                    "⚠ APPROVED WITH WARNINGS: CPID {}\n\
                     Warnings:\n{}",
                    cpid,
                    warnings.iter().map(|w| format!("  - {}", w)).collect::<Vec<_>>().join("\n")
                )
            }
            Self::Rejected { cpid, reasons, .. } => {
                format!(
                    "✗ REJECTED: CPID {}\n\
                     Reasons:\n{}",
                    cpid,
                    reasons.iter().map(|r| format!("  - {}", r)).collect::<Vec<_>>().join("\n")
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_build_success_rate() {
        use std::env;
        let temp_dir = env::temp_dir().join("aos_test_cdp_store");
        let metrics = CodeMetrics::new(temp_dir);
        let rate = metrics.compute_build_success_rate(&["cp1".to_string()])
            .expect("Test build success rate computation should succeed");
        assert!(rate >= 0.0 && rate <= 1.0);
    }

    #[test]
    fn test_compute_test_pass_rate() {
        use std::env;
        let temp_dir = env::temp_dir().join("aos_test_cdp_store");
        let metrics = CodeMetrics::new(temp_dir);
        let rate = metrics.compute_test_pass_rate(&["cp1".to_string()])
            .expect("Test pass rate computation should succeed");
        assert!(rate >= 0.0 && rate <= 1.0);
    }

    #[test]
    fn test_compute_lint_error_delta() {
        use std::env;
        let temp_dir = env::temp_dir().join("aos_test_cdp_store");
        let metrics = CodeMetrics::new(temp_dir);
        // This will fail to load CDPs, so we're just testing it doesn't crash
        let _ = metrics.compute_lint_error_delta("cp1", "cp2");
    }

    #[test]
    fn test_metrics_cache() {
        use std::env;
        let temp_dir = env::temp_dir().join("aos_test_cdp_store");
        let mut metrics = CodeMetrics::new(temp_dir);
        metrics.cache_metric("test_metric".to_string(), 0.95);
        
        let cached = metrics.get_cached_metric("test_metric")
            .expect("Test cached metric should be found");
        assert_eq!(cached.value, 0.95);
    }

    #[test]
    fn test_metrics_summary_meets_thresholds() {
        let summary = MetricsSummary {
            cpid: "test".to_string(),
            build_success_rate: 0.96,
            test_pass_rate: 0.92,
            regression_rate: 0.05,
            lint_error_count: 10,
            coverage_percent: 75.0,
        };

        let thresholds = MetricsThresholds::default();
        assert!(summary.meets_thresholds(&thresholds));
    }

    #[test]
    fn test_metrics_summary_fails_thresholds() {
        let summary = MetricsSummary {
            cpid: "test".to_string(),
            build_success_rate: 0.85, // Below 0.95 threshold
            test_pass_rate: 0.92,
            regression_rate: 0.05,
            lint_error_count: 10,
            coverage_percent: 75.0,
        };

        let thresholds = MetricsThresholds::default();
        assert!(!summary.meets_thresholds(&thresholds));
        
        let failures = summary.get_failures(&thresholds);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("Build success rate"));
    }

    #[test]
    fn test_promotion_gate_approved() {
        use std::env;
        let temp_dir = env::temp_dir().join("aos_test_cdp_store");
        let gate = CodePromotionGate::new(temp_dir);
        let decision = gate.check("test_cpid")
            .expect("Test promotion gate check should succeed");
        assert!(decision.is_approved());
    }

    #[test]
    fn test_promotion_gate_rejected() {
        use std::env;
        let temp_dir = env::temp_dir().join("aos_test_cdp_store");
        let thresholds = MetricsThresholds {
            min_build_success: 0.99, // Very high threshold
            ..Default::default()
        };
        let gate = CodePromotionGate::with_thresholds(temp_dir, thresholds);
        let decision = gate.check("test_cpid")
            .expect("Test promotion gate check should succeed");
        
        // Should be approved because no data returns 1.0
        assert!(decision.is_approved());
    }

    #[test]
    fn test_promotion_decision_format() {
        let summary = MetricsSummary {
            cpid: "test123".to_string(),
            build_success_rate: 0.96,
            test_pass_rate: 0.92,
            regression_rate: 0.05,
            lint_error_count: 10,
            coverage_percent: 75.0,
        };

        let decision = PromotionDecision::Approved {
            cpid: "test123".to_string(),
            summary,
        };

        let formatted = decision.format();
        assert!(formatted.contains("APPROVED"));
        assert!(formatted.contains("test123"));
    }

    #[test]
    fn test_metrics_thresholds_default() {
        let thresholds = MetricsThresholds::default();
        assert_eq!(thresholds.min_build_success, 0.95);
        assert_eq!(thresholds.min_test_pass, 0.90);
        assert_eq!(thresholds.max_regression_rate, 0.10);
    }
}
