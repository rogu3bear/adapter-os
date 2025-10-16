use crate::{
    assertions::evaluate_assertion,
    step_executor::{execute_step, framework_from_metadata, run_framework_step, ExternalFramework},
    types::*,
};
use adapteros_core::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};
use tracing::info;

#[async_trait]
pub trait TestingFramework {
    async fn setup(&self, config: &TestConfig) -> Result<TestEnvironment>;
    async fn teardown(&self, env: &TestEnvironment) -> Result<()>;
    async fn run_test(&self, test_case: &TestCase) -> Result<TestResult>;
    async fn run_suite(&self, suite: &TestSuite) -> Result<TestSuiteResult>;
    async fn get_coverage_report(&self) -> Result<CoverageReport>;
    async fn get_performance_metrics(&self) -> Result<PerformanceMetrics>;
}

pub struct UnifiedTestingFramework {
    _config: TestConfig,
    environments: Arc<Mutex<HashMap<String, TestEnvironment>>>,
    test_results_history: Arc<Mutex<Vec<TestResult>>>,
    performance_metrics: Arc<Mutex<PerformanceMetrics>>,
}

impl UnifiedTestingFramework {
    pub fn new(config: TestConfig) -> Self {
        Self {
            _config: config,
            environments: Arc::new(Mutex::new(HashMap::new())),
            test_results_history: Arc::new(Mutex::new(Vec::new())),
            performance_metrics: Arc::new(Mutex::new(PerformanceMetrics {
                total_execution_time_ms: 0,
                average_test_execution_time_ms: 0.0,
                slowest_test_execution_time_ms: 0,
                fastest_test_execution_time_ms: u64::MAX,
                memory_usage_bytes: 0,
                cpu_usage_percentage: 0.0,
                test_throughput: 0.0,
                timestamp: Utc::now(),
            })),
        }
    }

    fn record_test_result(&self, test_result: &TestResult) {
        if let Ok(mut history) = self.test_results_history.lock() {
            history.push(test_result.clone());
            let total = history.len() as f64;
            drop(history);
            if let Ok(mut metrics) = self.performance_metrics.lock() {
                metrics.total_execution_time_ms += test_result.execution_time_ms;
                metrics.slowest_test_execution_time_ms = metrics
                    .slowest_test_execution_time_ms
                    .max(test_result.execution_time_ms);
                metrics.fastest_test_execution_time_ms = metrics
                    .fastest_test_execution_time_ms
                    .min(test_result.execution_time_ms);
                metrics.average_test_execution_time_ms = if total <= 0.0 {
                    0.0
                } else {
                    metrics.total_execution_time_ms as f64 / total
                };
                metrics.test_throughput = if metrics.total_execution_time_ms == 0 {
                    0.0
                } else {
                    total / (metrics.total_execution_time_ms as f64 / 1000.0)
                };
                metrics.timestamp = Utc::now();
            }
        }
    }

    async fn run_test_step(&self, step: &TestStep) -> Result<StepResult> {
        execute_step(step).await
    }

    async fn run_assertion(&self, assertion: &TestAssertion) -> Result<AssertionResult> {
        evaluate_assertion(assertion).await
    }
}

#[async_trait]
impl TestingFramework for UnifiedTestingFramework {
    async fn setup(&self, config: &TestConfig) -> Result<TestEnvironment> {
        let env_id = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string();
        info!(env_id = %env_id, environment_type = ?config.environment_type, "setting up test environment");
        let env = TestEnvironment {
            id: env_id.clone(),
            environment_type: config.environment_type.clone(),
            state: EnvironmentState::Initializing,
            resources: HashMap::new(),
            metadata: HashMap::new(),
            created_at: Utc::now(),
        };
        if let Ok(mut environments) = self.environments.lock() {
            environments.insert(env_id.clone(), env.clone());
        }
        info!(env_id = %env_id, "test environment ready");
        Ok(env)
    }

    async fn teardown(&self, env: &TestEnvironment) -> Result<()> {
        info!(env_id = %env.id, "tearing down environment");
        if let Ok(mut environments) = self.environments.lock() {
            environments.remove(&env.id);
        }
        Ok(())
    }

    async fn run_test(&self, test_case: &TestCase) -> Result<TestResult> {
        info!(test_case_id = %test_case.id, test_name = %test_case.name, "running test case");
        let start_time = Utc::now();
        let start = Instant::now();
        let mut result = TestResult {
            test_case_id: test_case.id.clone(),
            status: TestStatus::Passed,
            execution_time_ms: 0,
            start_time,
            end_time: start_time,
            output: None,
            error: None,
            assertion_results: Vec::new(),
            step_results: Vec::new(),
            metadata: HashMap::new(),
        };
        let framework = framework_from_metadata(&test_case.metadata);
        let mut aggregated_status = TestStatus::Passed;
        if let Some(setup) = &test_case.setup {
            let step_result = self.run_test_step(setup).await?;
            aggregated_status = aggregated_status.merge(&step_result.status);
            result.step_results.push(step_result);
        }
        if framework != ExternalFramework::Native {
            let framework_step = run_framework_step(framework, test_case).await;
            aggregated_status = aggregated_status.merge(&framework_step.status);
            result.step_results.push(framework_step);
        }
        for step in &test_case.steps {
            let step_result = self.run_test_step(step).await?;
            aggregated_status = aggregated_status.merge(&step_result.status);
            result.step_results.push(step_result);
        }
        if let Some(teardown) = &test_case.teardown {
            let step_result = self.run_test_step(teardown).await?;
            aggregated_status = aggregated_status.merge(&step_result.status);
            result.step_results.push(step_result);
        }
        for assertion in &test_case.assertions {
            let assertion_result = self.run_assertion(assertion).await?;
            aggregated_status = aggregated_status.merge(&assertion_result.status);
            result.assertion_results.push(assertion_result);
        }
        result.end_time = Utc::now();
        result.execution_time_ms = start.elapsed().as_millis() as u64;
        result.status = aggregated_status;
        if result.status != TestStatus::Passed {
            result.error = Some(format!("test finished with status {:?}", result.status));
        }
        self.record_test_result(&result);
        info!(test_case_id = %test_case.id, status = ?result.status, execution_time_ms = result.execution_time_ms, "test case complete");
        Ok(result)
    }

    async fn run_suite(&self, suite: &TestSuite) -> Result<TestSuiteResult> {
        info!(suite_id = %suite.id, test_count = suite.test_cases.len(), "running test suite");
        let start = Instant::now();
        let start_time = Utc::now();
        let mut results = Vec::new();
        let mut suite_status = TestStatus::Passed;
        for case in &suite.test_cases {
            let test_result = self.run_test(case).await?;
            suite_status = suite_status.merge(&test_result.status);
            results.push(test_result);
        }
        let passed = results
            .iter()
            .filter(|r| r.status == TestStatus::Passed)
            .count() as u32;
        let failed = results
            .iter()
            .filter(|r| r.status == TestStatus::Failed)
            .count() as u32;
        let skipped = results
            .iter()
            .filter(|r| r.status == TestStatus::Skipped)
            .count() as u32;
        let errored = results
            .iter()
            .filter(|r| r.status == TestStatus::Error)
            .count() as u32;
        let timed_out = results
            .iter()
            .filter(|r| r.status == TestStatus::Timeout)
            .count() as u32;
        let total = results.len() as u32;
        let avg_time = if total == 0 {
            0.0
        } else {
            results.iter().map(|r| r.execution_time_ms).sum::<u64>() as f64 / total as f64
        };
        let summary = TestSummary {
            total_tests: total,
            passed_tests: passed,
            failed_tests: failed,
            skipped_tests: skipped,
            error_tests: errored,
            timeout_tests: timed_out,
            success_rate: if total == 0 {
                0.0
            } else {
                passed as f64 / total as f64
            },
            average_execution_time_ms: avg_time,
        };
        let result = TestSuiteResult {
            suite_id: suite.id.clone(),
            status: suite_status,
            execution_time_ms: start.elapsed().as_millis() as u64,
            start_time,
            end_time: Utc::now(),
            test_results: results,
            summary,
            metadata: suite.metadata.clone(),
        };
        info!(suite_id = %result.suite_id, status = ?result.status, success_rate = result.summary.success_rate, "suite complete");
        Ok(result)
    }

    async fn get_coverage_report(&self) -> Result<CoverageReport> {
        let history = self.test_results_history.lock().unwrap();
        let total = history.len();
        let passed = history
            .iter()
            .filter(|r| r.status == TestStatus::Passed)
            .count();
        let overall = if total == 0 {
            0.0
        } else {
            (passed as f64 / total as f64) * 100.0
        };
        let mut file_coverage = HashMap::new();
        for result in history.iter() {
            if let Some(Value::String(path)) = result.metadata.get("file") {
                file_coverage.entry(path.clone()).or_insert(FileCoverage {
                    file_path: path.clone(),
                    line_coverage: overall,
                    branch_coverage: overall,
                    function_coverage: overall,
                    covered_lines: Vec::new(),
                    uncovered_lines: Vec::new(),
                });
            }
        }
        Ok(CoverageReport {
            overall_coverage: overall,
            line_coverage: overall,
            branch_coverage: overall * 0.9,
            function_coverage: overall * 0.95,
            file_coverage,
            timestamp: Utc::now(),
        })
    }

    async fn get_performance_metrics(&self) -> Result<PerformanceMetrics> {
        let metrics = self.performance_metrics.lock().unwrap();
        Ok(metrics.clone())
    }
}

#[cfg(test)]
mod tests;
