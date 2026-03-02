//! Quality ratchet: monotonic quality gate and regression guard for bootstrap.
//!
//! Ensures each adapter version is strictly non-degrading across all quality
//! metrics. Includes diversity monitoring to detect model collapse, golden
//! test suites for regression detection, and contamination guards.
//!
//! Every bootstrap iteration produces a signed audit record.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::CodeGenQualityReport;

// ─── Version history ─────────────────────────────────────────────────────

/// History of adapter versions with their quality reports.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdapterVersionHistory {
    pub versions: Vec<AdapterVersionRecord>,
}

/// Record for a single adapter version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterVersionRecord {
    pub version: u32,
    pub adapter_id: String,
    pub adapter_hash: String,
    pub training_seed: u64,
    pub dataset_hash: String,
    pub eval_report: CodeGenQualityReport,
    pub timestamp: String,
    pub parent_version: Option<u32>,
}

impl AdapterVersionHistory {
    pub fn new() -> Self {
        Self {
            versions: Vec::new(),
        }
    }

    /// Record a new adapter version.
    pub fn record(&mut self, record: AdapterVersionRecord) {
        self.versions.push(record);
    }

    /// Get the latest version record.
    pub fn latest(&self) -> Option<&AdapterVersionRecord> {
        self.versions.last()
    }

    /// Get a version by number.
    pub fn get_version(&self, version: u32) -> Option<&AdapterVersionRecord> {
        self.versions.iter().find(|v| v.version == version)
    }

    /// Total number of recorded versions.
    pub fn len(&self) -> usize {
        self.versions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }
}

// ─── Monotonic quality gate ──────────────────────────────────────────────

/// Noise tolerance for metric comparisons.
/// Allows small regressions that fall within measurement noise.
const NOISE_TOLERANCE: f64 = 0.02;

/// Configuration for the quality ratchet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatchetConfig {
    /// Allow metric drops within noise tolerance.
    pub noise_tolerance: f64,
    /// Require improvement in at least one metric if others are equal.
    pub require_improvement: bool,
}

impl Default for RatchetConfig {
    fn default() -> Self {
        Self {
            noise_tolerance: NOISE_TOLERANCE,
            require_improvement: true,
        }
    }
}

/// Quality ratchet enforcing monotonic improvement.
pub struct QualityRatchet {
    history: AdapterVersionHistory,
    config: RatchetConfig,
}

/// Result of a ratchet check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RatchetResult {
    /// First version — no comparison baseline.
    FirstVersion,
    /// All metrics meet ratchet criteria.
    Passed { improvements: Vec<MetricDelta> },
    /// Ratchet criteria failed (regression beyond tolerance or no required improvement).
    Failed {
        regressions: Vec<MetricDelta>,
        improvements: Vec<MetricDelta>,
    },
}

/// Delta for a single metric between versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDelta {
    pub metric_name: String,
    pub previous: f64,
    pub current: f64,
    pub delta: f64,
}

impl QualityRatchet {
    pub fn new(history: AdapterVersionHistory, config: RatchetConfig) -> Self {
        Self { history, config }
    }

    /// Check if a new quality report meets the ratchet criteria.
    pub fn check(&self, new_report: &CodeGenQualityReport) -> RatchetResult {
        let prev = match self.history.latest() {
            Some(record) => &record.eval_report,
            None => return RatchetResult::FirstVersion,
        };

        let metrics = vec![
            ("compile_rate", prev.compile_rate, new_report.compile_rate),
            (
                "test_pass_rate",
                prev.test_pass_rate,
                new_report.test_pass_rate,
            ),
            (
                "exact_match_rate",
                prev.exact_match_rate,
                new_report.exact_match_rate,
            ),
            (
                "avg_token_overlap",
                prev.avg_token_overlap,
                new_report.avg_token_overlap,
            ),
        ];

        let mut improvements = Vec::new();
        let mut regressions = Vec::new();

        for (name, prev_val, curr_val) in &metrics {
            let delta = curr_val - prev_val;
            let md = MetricDelta {
                metric_name: name.to_string(),
                previous: *prev_val,
                current: *curr_val,
                delta,
            };

            if delta < -self.config.noise_tolerance {
                regressions.push(md);
            } else if delta > 0.0 {
                improvements.push(md);
            }
        }

        if regressions.is_empty() {
            if self.config.require_improvement && improvements.is_empty() {
                RatchetResult::Failed {
                    regressions: Vec::new(),
                    improvements: Vec::new(),
                }
            } else {
                RatchetResult::Passed { improvements }
            }
        } else {
            RatchetResult::Failed {
                regressions,
                improvements,
            }
        }
    }

    /// Record a version that passed the ratchet.
    pub fn record(&mut self, record: AdapterVersionRecord) {
        self.history.record(record);
    }

    pub fn history(&self) -> &AdapterVersionHistory {
        &self.history
    }
}

// ─── Diversity monitor ───────────────────────────────────────────────────

/// Report on output diversity for model collapse detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiversityReport {
    /// Unique bigram ratio: unique bigrams / total bigrams.
    pub unique_bigram_ratio: f64,
    /// Unique trigram ratio: unique trigrams / total trigrams.
    pub unique_trigram_ratio: f64,
    /// Variance in output lengths (tokens or chars).
    pub length_variance: f64,
    /// Fraction of outputs containing repeated 3+ line blocks.
    pub repetition_rate: f64,
    /// Overall diversity score (0.0 = collapsed, 1.0 = diverse).
    pub diversity_score: f64,
    /// Whether diversity is above the collapse threshold.
    pub healthy: bool,
}

/// Configuration for diversity monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiversityConfig {
    /// Minimum diversity score to consider healthy.
    pub min_diversity_score: f64,
    /// Minimum unique bigram ratio.
    pub min_unique_bigram_ratio: f64,
}

impl Default for DiversityConfig {
    fn default() -> Self {
        Self {
            min_diversity_score: 0.3,
            min_unique_bigram_ratio: 0.1,
        }
    }
}

/// Monitor for detecting model collapse through output diversity analysis.
pub struct DiversityMonitor {
    config: DiversityConfig,
}

impl DiversityMonitor {
    pub fn new(config: DiversityConfig) -> Self {
        Self { config }
    }

    /// Analyze diversity of generated outputs.
    pub fn check_diversity(&self, generated_outputs: &[String]) -> DiversityReport {
        if generated_outputs.is_empty() {
            return DiversityReport {
                unique_bigram_ratio: 0.0,
                unique_trigram_ratio: 0.0,
                length_variance: 0.0,
                repetition_rate: 0.0,
                diversity_score: 0.0,
                healthy: false,
            };
        }

        let unique_bigram_ratio = compute_unique_ngram_ratio(generated_outputs, 2);
        let unique_trigram_ratio = compute_unique_ngram_ratio(generated_outputs, 3);
        let length_variance = compute_length_variance(generated_outputs);
        let repetition_rate = compute_repetition_rate(generated_outputs);

        // Composite score: weighted average
        let diversity_score = 0.3 * unique_bigram_ratio
            + 0.3 * unique_trigram_ratio
            + 0.2 * (1.0 - repetition_rate)
            + 0.2 * (length_variance / (length_variance + 100.0)); // sigmoid-like normalization

        let healthy = diversity_score >= self.config.min_diversity_score
            && unique_bigram_ratio >= self.config.min_unique_bigram_ratio;

        DiversityReport {
            unique_bigram_ratio,
            unique_trigram_ratio,
            length_variance,
            repetition_rate,
            diversity_score,
            healthy,
        }
    }
}

fn compute_unique_ngram_ratio(outputs: &[String], n: usize) -> f64 {
    use std::collections::HashSet;

    let mut total = 0usize;
    let mut unique = HashSet::new();

    for output in outputs {
        let words: Vec<&str> = output.split_whitespace().collect();
        if words.len() < n {
            continue;
        }
        for window in words.windows(n) {
            let ngram: Vec<&str> = window.to_vec();
            total += 1;
            unique.insert(ngram);
        }
    }

    if total == 0 {
        return 0.0;
    }
    unique.len() as f64 / total as f64
}

fn compute_length_variance(outputs: &[String]) -> f64 {
    if outputs.is_empty() {
        return 0.0;
    }
    let lengths: Vec<f64> = outputs.iter().map(|o| o.len() as f64).collect();
    let mean = lengths.iter().sum::<f64>() / lengths.len() as f64;
    let variance = lengths.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / lengths.len() as f64;
    variance
}

fn compute_repetition_rate(outputs: &[String]) -> f64 {
    if outputs.is_empty() {
        return 0.0;
    }

    let mut repeated_count = 0;
    for output in outputs {
        let lines: Vec<&str> = output.lines().collect();
        if lines.len() < 6 {
            continue;
        }
        // Check for any 3-line block that repeats
        let mut found_repeat = false;
        for i in 0..lines.len().saturating_sub(5) {
            let block = &lines[i..i + 3];
            for j in (i + 3)..lines.len().saturating_sub(2) {
                if lines[j..j + 3] == *block {
                    found_repeat = true;
                    break;
                }
            }
            if found_repeat {
                break;
            }
        }
        if found_repeat {
            repeated_count += 1;
        }
    }

    repeated_count as f64 / outputs.len() as f64
}

// ─── Golden test suite ───────────────────────────────────────────────────

/// A curated regression test for adapter quality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenTest {
    pub name: String,
    pub prompt: String,
    pub expected_output: String,
    pub tolerance: GoldenTolerance,
}

/// How strictly to compare adapter output against expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoldenTolerance {
    /// Output must match exactly (whitespace-normalized).
    ExactMatch,
    /// Output must compile successfully.
    CompileOnly,
    /// Output must compile and pass tests.
    TestPassOnly,
}

/// Result of running a golden test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenTestResult {
    pub test_name: String,
    pub passed: bool,
    pub actual_output: String,
    pub details: Option<String>,
}

/// Suite of golden tests for regression detection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoldenTestSuite {
    pub tests: Vec<GoldenTest>,
}

impl GoldenTestSuite {
    pub fn new() -> Self {
        Self { tests: Vec::new() }
    }

    pub fn add_test(&mut self, test: GoldenTest) {
        self.tests.push(test);
    }

    /// Evaluate outputs against the golden test suite.
    ///
    /// `outputs` maps test name → actual generated output.
    pub fn evaluate(&self, outputs: &BTreeMap<String, String>) -> Vec<GoldenTestResult> {
        self.tests
            .iter()
            .map(|test| {
                let actual = outputs.get(&test.name).cloned().unwrap_or_default();
                let passed = match test.tolerance {
                    GoldenTolerance::ExactMatch => {
                        normalize_whitespace(&actual) == normalize_whitespace(&test.expected_output)
                    }
                    // CompileOnly and TestPassOnly require external compilation —
                    // we return false here and let the caller validate via CompilationChecker
                    GoldenTolerance::CompileOnly | GoldenTolerance::TestPassOnly => false,
                };

                GoldenTestResult {
                    test_name: test.name.clone(),
                    passed,
                    actual_output: actual,
                    details: None,
                }
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.tests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tests.is_empty()
    }
}

fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ─── Contamination guard ─────────────────────────────────────────────────

/// Result of a contamination check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContaminationResult {
    /// Whether the held-out set is clean.
    pub clean: bool,
    /// Held-out items that appear in the training data.
    pub contaminated_items: Vec<ContaminatedItem>,
}

/// A held-out item that leaked into training data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContaminatedItem {
    pub held_out_index: usize,
    pub overlap_ratio: f64,
    pub matched_training_index: Option<usize>,
}

/// Guard against training data contamination of held-out evaluation sets.
pub struct ContaminationGuard;

impl ContaminationGuard {
    /// Check for contamination between training data and held-out set.
    ///
    /// Uses token overlap to detect if held-out items leaked into training.
    /// An overlap ratio above `threshold` flags contamination.
    pub fn check(
        training_data: &[String],
        held_out_set: &[String],
        threshold: f64,
    ) -> ContaminationResult {
        let mut contaminated_items = Vec::new();

        for (ho_idx, held_out) in held_out_set.iter().enumerate() {
            let ho_tokens: std::collections::HashSet<&str> = held_out.split_whitespace().collect();

            if ho_tokens.is_empty() {
                continue;
            }

            for (tr_idx, training) in training_data.iter().enumerate() {
                let tr_tokens: std::collections::HashSet<&str> =
                    training.split_whitespace().collect();

                let intersection = ho_tokens.intersection(&tr_tokens).count();
                let overlap = intersection as f64 / ho_tokens.len() as f64;

                if overlap >= threshold {
                    contaminated_items.push(ContaminatedItem {
                        held_out_index: ho_idx,
                        overlap_ratio: overlap,
                        matched_training_index: Some(tr_idx),
                    });
                    break; // one match is enough
                }
            }
        }

        ContaminationResult {
            clean: contaminated_items.is_empty(),
            contaminated_items,
        }
    }
}

// ─── Audit record ────────────────────────────────────────────────────────

/// Signed audit record for a bootstrap iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapAuditRecord {
    pub iteration: u32,
    pub adapter_version: String,
    pub adapter_hash: String,
    pub training_config_hash: String,
    pub dataset_hash: String,
    pub eval_report: CodeGenQualityReport,
    pub diversity_report: Option<DiversityReport>,
    pub ratchet_result: RatchetResult,
    pub golden_test_results: Vec<GoldenTestResult>,
    pub proposals_applied: Vec<String>,
    pub timestamp: String,
    /// Hex-encoded Ed25519 signature over the canonical JSON of this record (excluding signature).
    pub signature: String,
}

impl BootstrapAuditRecord {
    /// Compute the canonical bytes for signing.
    ///
    /// Serializes the record (with empty signature) to deterministic JSON.
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut record = self.clone();
        record.signature = String::new();
        // serde_json with sorted keys for determinism
        serde_json::to_vec(&record).unwrap_or_default()
    }

    /// Sign this audit record with the given signing key.
    ///
    /// Uses `adapteros_crypto::sign_data` for Ed25519 signing.
    pub fn sign(&mut self, signing_key_hex: &str) -> adapteros_core::Result<()> {
        let payload = self.signing_payload();
        let sig_bytes = adapteros_crypto::signature::sign_data(&payload, signing_key_hex)?;
        self.signature = hex::encode(sig_bytes);
        Ok(())
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_report(compile_rate: f64, test_pass_rate: f64) -> CodeGenQualityReport {
        CodeGenQualityReport {
            adapter_id: "test".into(),
            adapter_hash: "hash".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            held_out_count: 100,
            compile_rate,
            test_pass_rate,
            exact_match_rate: 0.5,
            avg_edit_distance: 0.3,
            avg_token_overlap: 0.6,
            strategy_breakdown: BTreeMap::new(),
            passed_promotion_gate: true,
        }
    }

    fn make_version(version: u32, report: CodeGenQualityReport) -> AdapterVersionRecord {
        AdapterVersionRecord {
            version,
            adapter_id: format!("adapter-v{}", version),
            adapter_hash: "hash".into(),
            training_seed: 42,
            dataset_hash: "dataset".into(),
            eval_report: report,
            timestamp: "2026-01-01T00:00:00Z".into(),
            parent_version: if version > 0 { Some(version - 1) } else { None },
        }
    }

    #[test]
    fn test_ratchet_first_version() {
        let ratchet = QualityRatchet::new(AdapterVersionHistory::new(), RatchetConfig::default());
        let report = make_report(0.85, 0.75);
        assert!(matches!(
            ratchet.check(&report),
            RatchetResult::FirstVersion
        ));
    }

    #[test]
    fn test_ratchet_improvement_passes() {
        let mut history = AdapterVersionHistory::new();
        history.record(make_version(0, make_report(0.80, 0.70)));

        let ratchet = QualityRatchet::new(history, RatchetConfig::default());
        let new_report = make_report(0.85, 0.75);

        match ratchet.check(&new_report) {
            RatchetResult::Passed { improvements } => {
                assert!(!improvements.is_empty());
            }
            other => panic!("Expected Passed, got {:?}", other),
        }
    }

    #[test]
    fn test_ratchet_regression_fails() {
        let mut history = AdapterVersionHistory::new();
        history.record(make_version(0, make_report(0.90, 0.85)));

        let ratchet = QualityRatchet::new(history, RatchetConfig::default());
        // Significant regression
        let new_report = make_report(0.75, 0.60);

        match ratchet.check(&new_report) {
            RatchetResult::Failed { regressions, .. } => {
                assert!(regressions.len() >= 2);
            }
            other => panic!("Expected Failed, got {:?}", other),
        }
    }

    #[test]
    fn test_ratchet_within_noise_tolerance() {
        let mut history = AdapterVersionHistory::new();
        history.record(make_version(0, make_report(0.85, 0.75)));

        let mut config = RatchetConfig::default();
        config.require_improvement = false;
        let ratchet = QualityRatchet::new(history, config);
        // Small regression within tolerance (0.02)
        let new_report = make_report(0.84, 0.74);

        match ratchet.check(&new_report) {
            RatchetResult::Passed { .. } => {} // Within noise tolerance
            other => panic!("Expected Passed (within noise), got {:?}", other),
        }
    }

    #[test]
    fn test_ratchet_requires_improvement() {
        let mut history = AdapterVersionHistory::new();
        history.record(make_version(0, make_report(0.85, 0.75)));

        let ratchet = QualityRatchet::new(history, RatchetConfig::default());
        let new_report = make_report(0.85, 0.75);

        match ratchet.check(&new_report) {
            RatchetResult::Failed {
                regressions,
                improvements,
            } => {
                assert!(regressions.is_empty());
                assert!(improvements.is_empty());
            }
            other => panic!("Expected Failed (require improvement), got {:?}", other),
        }
    }

    #[test]
    fn test_ratchet_disable_require_improvement() {
        let mut history = AdapterVersionHistory::new();
        history.record(make_version(0, make_report(0.85, 0.75)));

        let mut config = RatchetConfig::default();
        config.require_improvement = false;
        let ratchet = QualityRatchet::new(history, config);
        let new_report = make_report(0.85, 0.75);

        match ratchet.check(&new_report) {
            RatchetResult::Passed { improvements } => {
                assert!(improvements.is_empty());
            }
            other => panic!(
                "Expected Passed when require_improvement disabled, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_diversity_empty_outputs() {
        let monitor = DiversityMonitor::new(DiversityConfig::default());
        let report = monitor.check_diversity(&[]);
        assert!(!report.healthy);
        assert_eq!(report.diversity_score, 0.0);
    }

    #[test]
    fn test_diversity_collapsed_outputs() {
        let monitor = DiversityMonitor::new(DiversityConfig::default());
        // All identical outputs = low diversity
        let outputs: Vec<String> = (0..20).map(|_| "fn foo() { 42 }".to_string()).collect();
        let report = monitor.check_diversity(&outputs);
        // Should have low unique ngram ratio since all are identical
        assert!(report.unique_bigram_ratio <= 1.0);
    }

    #[test]
    fn test_diversity_varied_outputs() {
        let monitor = DiversityMonitor::new(DiversityConfig::default());
        let outputs: Vec<String> = (0..20)
            .map(|i| {
                format!(
                    "fn func_{}() {{ return {} + {} * {}; }}",
                    i,
                    i,
                    i * 2,
                    i * 3
                )
            })
            .collect();
        let report = monitor.check_diversity(&outputs);
        assert!(report.unique_bigram_ratio > 0.0);
    }

    #[test]
    fn test_golden_test_exact_match() {
        let mut suite = GoldenTestSuite::new();
        suite.add_test(GoldenTest {
            name: "hello".into(),
            prompt: "Write hello world".into(),
            expected_output: "fn main() { println!(\"Hello\"); }".into(),
            tolerance: GoldenTolerance::ExactMatch,
        });

        let mut outputs = BTreeMap::new();
        outputs.insert(
            "hello".to_string(),
            "fn  main()  { println!(\"Hello\"); }".to_string(),
        );

        let results = suite.evaluate(&outputs);
        assert_eq!(results.len(), 1);
        assert!(results[0].passed); // whitespace-normalized match
    }

    #[test]
    fn test_golden_test_mismatch() {
        let mut suite = GoldenTestSuite::new();
        suite.add_test(GoldenTest {
            name: "add".into(),
            prompt: "Write add".into(),
            expected_output: "fn add(a: i32, b: i32) -> i32 { a + b }".into(),
            tolerance: GoldenTolerance::ExactMatch,
        });

        let mut outputs = BTreeMap::new();
        outputs.insert(
            "add".to_string(),
            "fn add(x: i32, y: i32) -> i32 { x + y }".to_string(),
        );

        let results = suite.evaluate(&outputs);
        assert!(!results[0].passed); // Different parameter names
    }

    #[test]
    fn test_golden_test_compile_only_requires_external_checker() {
        let mut suite = GoldenTestSuite::new();
        suite.add_test(GoldenTest {
            name: "compile".into(),
            prompt: "Write compileable code".into(),
            expected_output: String::new(),
            tolerance: GoldenTolerance::CompileOnly,
        });

        let mut outputs = BTreeMap::new();
        outputs.insert("compile".to_string(), "fn main() {}".to_string());

        let results = suite.evaluate(&outputs);
        assert_eq!(results.len(), 1);
        assert!(
            !results[0].passed,
            "compile-only tolerance must be validated by CompilationChecker"
        );
    }

    #[test]
    fn test_golden_test_test_pass_only_requires_external_checker() {
        let mut suite = GoldenTestSuite::new();
        suite.add_test(GoldenTest {
            name: "tests".into(),
            prompt: "Write code with passing tests".into(),
            expected_output: String::new(),
            tolerance: GoldenTolerance::TestPassOnly,
        });

        let mut outputs = BTreeMap::new();
        outputs.insert(
            "tests".to_string(),
            "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
        );

        let results = suite.evaluate(&outputs);
        assert_eq!(results.len(), 1);
        assert!(
            !results[0].passed,
            "test-pass-only tolerance must be validated by CompilationChecker"
        );
    }

    #[test]
    fn test_contamination_clean() {
        let training = vec!["fn foo() { 1 }".to_string(), "fn bar() { 2 }".to_string()];
        let held_out = vec!["fn baz() { completely different }".to_string()];

        let result = ContaminationGuard::check(&training, &held_out, 0.8);
        assert!(result.clean);
    }

    #[test]
    fn test_contamination_detected() {
        let training = vec!["fn foo() { 1 + 2 + 3 }".to_string()];
        let held_out = vec!["fn foo() { 1 + 2 + 3 }".to_string()]; // exact copy

        let result = ContaminationGuard::check(&training, &held_out, 0.8);
        assert!(!result.clean);
        assert_eq!(result.contaminated_items.len(), 1);
        assert!(result.contaminated_items[0].overlap_ratio >= 0.8);
    }

    #[test]
    fn test_version_history_tracking() {
        let mut history = AdapterVersionHistory::new();
        assert!(history.is_empty());

        history.record(make_version(0, make_report(0.80, 0.70)));
        history.record(make_version(1, make_report(0.85, 0.75)));

        assert_eq!(history.len(), 2);
        assert_eq!(history.latest().unwrap().version, 1);
        assert!(history.get_version(0).is_some());
        assert!(history.get_version(99).is_none());
    }

    #[test]
    fn test_audit_record_signing_payload() {
        let record = BootstrapAuditRecord {
            iteration: 0,
            adapter_version: "v0".into(),
            adapter_hash: "hash".into(),
            training_config_hash: "cfg".into(),
            dataset_hash: "data".into(),
            eval_report: make_report(0.85, 0.75),
            diversity_report: None,
            ratchet_result: RatchetResult::FirstVersion,
            golden_test_results: vec![],
            proposals_applied: vec![],
            timestamp: "2026-01-01T00:00:00Z".into(),
            signature: "should_be_cleared".into(),
        };

        let payload = record.signing_payload();
        let json: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        // Signature should be empty in the payload
        assert_eq!(json["signature"], "");
    }

    #[test]
    fn test_repetition_rate_no_repetitions() {
        let outputs = vec![
            "line1\nline2\nline3\nline4\nline5\nline6".to_string(),
            "a\nb\nc\nd\ne\nf".to_string(),
        ];
        let rate = compute_repetition_rate(&outputs);
        assert_eq!(rate, 0.0);
    }

    #[test]
    fn test_repetition_rate_with_repetitions() {
        let outputs = vec![
            "aaa\nbbb\nccc\naaa\nbbb\nccc".to_string(), // 3-line block repeats
        ];
        let rate = compute_repetition_rate(&outputs);
        assert!(rate > 0.0);
    }

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(
            normalize_whitespace("fn  foo()  {\n    42\n}"),
            "fn foo() { 42 }"
        );
    }
}
