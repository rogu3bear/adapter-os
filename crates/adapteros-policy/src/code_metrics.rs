//! Code intelligence metrics for evaluation

use serde::{Deserialize, Serialize};

/// Compile Success Rate metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileSuccessRate {
    pub total_attempts: usize,
    pub successful_compiles: usize,
    pub rate: f32,
}

impl CompileSuccessRate {
    /// Create a new CSR tracker
    pub fn new() -> Self {
        Self {
            total_attempts: 0,
            successful_compiles: 0,
            rate: 0.0,
        }
    }

    /// Record a compilation attempt
    pub fn record(&mut self, success: bool) {
        self.total_attempts += 1;
        if success {
            self.successful_compiles += 1;
        }
        self.update_rate();
    }

    /// Update the rate calculation
    fn update_rate(&mut self) {
        if self.total_attempts > 0 {
            self.rate = self.successful_compiles as f32 / self.total_attempts as f32;
        }
    }

    /// Get the current rate
    pub fn get_rate(&self) -> f32 {
        self.rate
    }

    /// Check if rate meets threshold
    pub fn meets_threshold(&self, threshold: f32) -> bool {
        self.rate >= threshold
    }
}

impl Default for CompileSuccessRate {
    fn default() -> Self {
        Self::new()
    }
}

/// Test Pass@1 metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestPass1 {
    pub total_test_runs: usize,
    pub passed_first_try: usize,
    pub rate: f32,
}

impl TestPass1 {
    /// Create a new Test Pass@1 tracker
    pub fn new() -> Self {
        Self {
            total_test_runs: 0,
            passed_first_try: 0,
            rate: 0.0,
        }
    }

    /// Record a test run
    pub fn record(&mut self, passed_first_try: bool) {
        self.total_test_runs += 1;
        if passed_first_try {
            self.passed_first_try += 1;
        }
        self.update_rate();
    }

    /// Update the rate calculation
    fn update_rate(&mut self) {
        if self.total_test_runs > 0 {
            self.rate = self.passed_first_try as f32 / self.total_test_runs as f32;
        }
    }

    /// Get the current rate
    pub fn get_rate(&self) -> f32 {
        self.rate
    }

    /// Check if rate meets threshold
    pub fn meets_threshold(&self, threshold: f32) -> bool {
        self.rate >= threshold
    }
}

impl Default for TestPass1 {
    fn default() -> Self {
        Self::new()
    }
}

/// Answer Relevance Rate (ARR) metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerRelevanceRate {
    pub total_responses: usize,
    pub responses_with_citations: usize,
    pub rate: f32,
    pub min_citations_required: usize,
}

impl AnswerRelevanceRate {
    /// Create a new ARR tracker
    pub fn new(min_citations_required: usize) -> Self {
        Self {
            total_responses: 0,
            responses_with_citations: 0,
            rate: 0.0,
            min_citations_required,
        }
    }

    /// Record a response with citation count
    pub fn record(&mut self, citation_count: usize) {
        self.total_responses += 1;
        if citation_count >= self.min_citations_required {
            self.responses_with_citations += 1;
        }
        self.update_rate();
    }

    /// Record a response with citations
    pub fn record_with_citations(&mut self, has_citations: bool) {
        self.total_responses += 1;
        if has_citations {
            self.responses_with_citations += 1;
        }
        self.update_rate();
    }

    /// Update the rate calculation
    fn update_rate(&mut self) {
        if self.total_responses > 0 {
            self.rate = self.responses_with_citations as f32 / self.total_responses as f32;
        }
    }

    /// Get the current rate
    pub fn get_rate(&self) -> f32 {
        self.rate
    }

    /// Check if rate meets threshold
    pub fn meets_threshold(&self, threshold: f32) -> bool {
        self.rate >= threshold
    }
}

impl Default for AnswerRelevanceRate {
    fn default() -> Self {
        Self::new(1)
    }
}

/// Code metrics aggregator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeMetrics {
    pub csr: CompileSuccessRate,
    pub test_pass1: TestPass1,
    pub arr: AnswerRelevanceRate,
}

impl CodeMetrics {
    /// Create a new code metrics tracker
    pub fn new(min_citations: usize) -> Self {
        Self {
            csr: CompileSuccessRate::new(),
            test_pass1: TestPass1::new(),
            arr: AnswerRelevanceRate::new(min_citations),
        }
    }

    /// Record compilation result
    pub fn record_compile(&mut self, success: bool) {
        self.csr.record(success);
    }

    /// Record test result
    pub fn record_test(&mut self, passed_first_try: bool) {
        self.test_pass1.record(passed_first_try);
    }

    /// Record response with citations
    pub fn record_response(&mut self, citation_count: usize) {
        self.arr.record(citation_count);
    }

    /// Get metrics summary
    pub fn summary(&self) -> MetricsSummary {
        MetricsSummary {
            csr: self.csr.get_rate(),
            test_pass1: self.test_pass1.get_rate(),
            arr: self.arr.get_rate(),
            total_compiles: self.csr.total_attempts,
            total_tests: self.test_pass1.total_test_runs,
            total_responses: self.arr.total_responses,
        }
    }

    /// Check if all metrics meet thresholds
    pub fn meets_thresholds(
        &self,
        csr_threshold: f32,
        test_threshold: f32,
        arr_threshold: f32,
    ) -> bool {
        self.csr.meets_threshold(csr_threshold)
            && self.test_pass1.meets_threshold(test_threshold)
            && self.arr.meets_threshold(arr_threshold)
    }
}

impl Default for CodeMetrics {
    fn default() -> Self {
        Self::new(1)
    }
}

/// Metrics summary for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub csr: f32,
    pub test_pass1: f32,
    pub arr: f32,
    pub total_compiles: usize,
    pub total_tests: usize,
    pub total_responses: usize,
}

impl MetricsSummary {
    /// Format as human-readable string
    pub fn format(&self) -> String {
        format!(
            "Code Metrics Summary:\n\
             CSR (Compile Success Rate): {:.2}% ({}/{})\n\
             Test Pass@1: {:.2}% ({}/{})\n\
             ARR (Answer Relevance Rate): {:.2}% ({}/{})",
            self.csr * 100.0,
            (self.csr * self.total_compiles as f32) as usize,
            self.total_compiles,
            self.test_pass1 * 100.0,
            (self.test_pass1 * self.total_tests as f32) as usize,
            self.total_tests,
            self.arr * 100.0,
            (self.arr * self.total_responses as f32) as usize,
            self.total_responses,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csr_tracking() {
        let mut csr = CompileSuccessRate::new();

        csr.record(true);
        csr.record(true);
        csr.record(false);

        assert_eq!(csr.total_attempts, 3);
        assert_eq!(csr.successful_compiles, 2);
        assert!((csr.get_rate() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_test_pass1() {
        let mut tp1 = TestPass1::new();

        tp1.record(true);
        tp1.record(false);
        tp1.record(true);
        tp1.record(true);

        assert_eq!(tp1.total_test_runs, 4);
        assert_eq!(tp1.passed_first_try, 3);
        assert_eq!(tp1.get_rate(), 0.75);
    }

    #[test]
    fn test_arr_with_min_citations() {
        let mut arr = AnswerRelevanceRate::new(2);

        arr.record(0); // No citations
        arr.record(1); // 1 citation (below min)
        arr.record(2); // 2 citations (meets min)
        arr.record(3); // 3 citations (exceeds min)

        assert_eq!(arr.total_responses, 4);
        assert_eq!(arr.responses_with_citations, 2);
        assert_eq!(arr.get_rate(), 0.5);
    }

    #[test]
    fn test_code_metrics_aggregation() {
        let mut metrics = CodeMetrics::new(1);

        metrics.record_compile(true);
        metrics.record_compile(true);
        metrics.record_test(true);
        metrics.record_response(2);

        let summary = metrics.summary();
        assert_eq!(summary.csr, 1.0);
        assert_eq!(summary.test_pass1, 1.0);
        assert_eq!(summary.arr, 1.0);
    }

    #[test]
    fn test_threshold_checking() {
        let mut metrics = CodeMetrics::new(1);

        // Add some mixed results
        metrics.record_compile(true);
        metrics.record_compile(true);
        metrics.record_compile(false);

        metrics.record_test(true);
        metrics.record_test(true);

        metrics.record_response(2);
        metrics.record_response(1);
        metrics.record_response(3);

        // Check thresholds
        assert!(metrics.meets_thresholds(0.6, 0.9, 0.9));
        assert!(!metrics.meets_thresholds(0.8, 0.9, 0.9));
    }

    #[test]
    fn test_summary_formatting() {
        let mut metrics = CodeMetrics::new(1);

        metrics.record_compile(true);
        metrics.record_compile(false);
        metrics.record_test(true);
        metrics.record_response(2);

        let summary = metrics.summary();
        let formatted = summary.format();

        assert!(formatted.contains("CSR"));
        assert!(formatted.contains("Test Pass@1"));
        assert!(formatted.contains("ARR"));
        assert!(formatted.contains("50.00%"));
    }
}
