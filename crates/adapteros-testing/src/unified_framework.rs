//! Unified testing framework for adapterOS
//!
//! Provides a centralized testing framework that consolidates all testing
//! patterns across the system with consistent setup, teardown, and assertions.
//!
//! # Citations
//! - CONTRIBUTING.md L118-122: "Follow Rust naming conventions", "Use `cargo clippy` for linting"
//! - AGENTS.md L50-55: "Testing frameworks with deterministic execution"

use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Type alias for test step results used internally
pub type TestStepResult = StepResult;

/// Unified testing framework interface
#[async_trait]
pub trait TestingFramework {
    /// Setup test environment
    async fn setup(&self, config: &TestConfig) -> Result<TestEnvironment>;

    /// Teardown test environment
    async fn teardown(&self, env: &TestEnvironment) -> Result<()>;

    /// Run a test case
    async fn run_test(&self, test_case: &TestCase) -> Result<TestResult>;

    /// Run a test suite
    async fn run_suite(&self, suite: &TestSuite) -> Result<TestSuiteResult>;

    /// Get test coverage report
    async fn get_coverage_report(&self) -> Result<CoverageReport>;

    /// Get test performance metrics
    async fn get_performance_metrics(&self) -> Result<PerformanceMetrics>;
}

/// Test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    /// Test environment type
    pub environment_type: TestEnvironmentType,

    /// Test timeout in seconds
    pub timeout_seconds: u64,

    /// Maximum concurrent tests
    pub max_concurrent_tests: u32,

    /// Enable test isolation
    pub enable_isolation: bool,

    /// Enable test parallelization
    pub enable_parallelization: bool,

    /// Test data directory
    pub test_data_dir: Option<String>,

    /// Test fixtures directory
    pub fixtures_dir: Option<String>,

    /// Additional configuration
    pub additional_config: HashMap<String, serde_json::Value>,
}

/// Test environment types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestEnvironmentType {
    /// Unit test environment
    Unit,

    /// Integration test environment
    Integration,

    /// End-to-end test environment
    EndToEnd,

    /// Performance test environment
    Performance,

    /// Security test environment
    Security,

    /// Determinism test environment
    Determinism,
}

/// Test environment
#[derive(Debug, Clone)]
pub struct TestEnvironment {
    /// Environment identifier
    pub id: String,

    /// Environment type
    pub environment_type: TestEnvironmentType,

    /// Environment state
    pub state: EnvironmentState,

    /// Environment resources
    pub resources: HashMap<String, serde_json::Value>,

    /// Environment metadata
    pub metadata: HashMap<String, String>,

    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Environment states
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EnvironmentState {
    /// Initializing
    Initializing,

    /// Ready
    Ready,

    /// Running
    Running,

    /// Cleaning up
    CleaningUp,

    /// Failed
    Failed,

    /// Destroyed
    Destroyed,
}

/// Test case
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    /// Test case identifier
    pub id: String,

    /// Test case name
    pub name: String,

    /// Test case description
    pub description: Option<String>,

    /// Test case type
    pub test_type: TestType,

    /// Test case priority
    pub priority: TestPriority,

    /// Test case tags
    pub tags: Vec<String>,

    /// Test case setup
    pub setup: Option<TestStep>,

    /// Test case steps
    pub steps: Vec<TestStep>,

    /// Test case teardown
    pub teardown: Option<TestStep>,

    /// Test case assertions
    pub assertions: Vec<TestAssertion>,

    /// Test case timeout
    pub timeout_seconds: Option<u64>,

    /// Test case dependencies
    pub dependencies: Vec<String>,

    /// Test case metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Test types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestType {
    /// Unit test
    Unit,

    /// Integration test
    Integration,

    /// End-to-end test
    EndToEnd,

    /// Performance test
    Performance,

    /// Security test
    Security,

    /// Determinism test
    Determinism,

    /// Regression test
    Regression,

    /// Smoke test
    Smoke,
}

/// Test priorities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestPriority {
    /// Critical priority
    Critical,

    /// High priority
    High,

    /// Medium priority
    Medium,

    /// Low priority
    Low,
}

/// Test step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStep {
    /// Step identifier
    pub id: String,

    /// Step name
    pub name: String,

    /// Step description
    pub description: Option<String>,

    /// Step action
    pub action: TestAction,

    /// Step parameters
    pub parameters: HashMap<String, serde_json::Value>,

    /// Step timeout
    pub timeout_seconds: Option<u64>,

    /// Step retries
    pub retries: Option<u32>,

    /// Step dependencies
    pub dependencies: Vec<String>,
}

/// Test actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestAction {
    /// Execute command
    ExecuteCommand { command: String, args: Vec<String> },

    /// Make API call
    ApiCall {
        method: String,
        url: String,
        body: Option<serde_json::Value>,
    },

    /// Database operation
    DatabaseOperation {
        operation: String,
        query: String,
        params: Vec<serde_json::Value>,
    },

    /// File operation
    FileOperation {
        operation: String,
        path: String,
        content: Option<String>,
    },

    /// Network operation
    NetworkOperation {
        operation: String,
        host: String,
        port: u16,
    },

    /// Wait action
    Wait { duration_ms: u64 },

    /// Assert action
    Assert {
        condition: String,
        expected: serde_json::Value,
    },

    /// Custom action
    Custom {
        action_type: String,
        data: serde_json::Value,
    },
}

/// Test assertion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestAssertion {
    /// Assertion identifier
    pub id: String,

    /// Assertion name
    pub name: String,

    /// Assertion type
    pub assertion_type: AssertionType,

    /// Assertion parameters
    pub parameters: HashMap<String, serde_json::Value>,

    /// Assertion message
    pub message: Option<String>,
}

/// Assertion types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssertionType {
    /// Equality assertion
    Equals,

    /// Not equals assertion
    NotEquals,

    /// Greater than assertion
    GreaterThan,

    /// Less than assertion
    LessThan,

    /// Contains assertion
    Contains,

    /// Not contains assertion
    NotContains,

    /// Regex match assertion
    RegexMatch,

    /// File exists assertion
    FileExists,

    /// File not exists assertion
    FileNotExists,

    /// Database record exists assertion
    DatabaseRecordExists,

    /// API response assertion
    ApiResponse,

    /// JSON path assertion
    JsonPath,

    /// Custom assertion
    Custom { assertion_type: String },
}

/// Test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Test case identifier
    pub test_case_id: String,

    /// Test result status
    pub status: TestStatus,

    /// Test execution time in milliseconds
    pub execution_time_ms: u64,

    /// Test start time
    pub start_time: chrono::DateTime<chrono::Utc>,

    /// Test end time
    pub end_time: chrono::DateTime<chrono::Utc>,

    /// Test output
    pub output: Option<String>,

    /// Test error
    pub error: Option<String>,

    /// Test assertions results
    pub assertion_results: Vec<AssertionResult>,

    /// Test step results
    pub step_results: Vec<StepResult>,

    /// Test metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Test statuses
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TestStatus {
    /// Test passed
    Passed,

    /// Test failed
    Failed,

    /// Test skipped
    Skipped,

    /// Test error
    Error,

    /// Test timeout
    Timeout,
}

/// Assertion result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionResult {
    /// Assertion identifier
    pub assertion_id: String,

    /// Assertion status
    pub status: TestStatus,

    /// Assertion message
    pub message: Option<String>,

    /// Assertion details
    pub details: Option<serde_json::Value>,
}

/// Step result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step identifier
    pub step_id: String,

    /// Step status
    pub status: TestStatus,

    /// Step output
    pub output: Option<String>,

    /// Step error
    pub error: Option<String>,

    /// Step execution time in milliseconds
    pub execution_time_ms: u64,
}

/// Test suite
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    /// Suite identifier
    pub id: String,

    /// Suite name
    pub name: String,

    /// Suite description
    pub description: Option<String>,

    /// Suite test cases
    pub test_cases: Vec<TestCase>,

    /// Suite configuration
    pub config: TestConfig,

    /// Suite metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Test suite result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuiteResult {
    /// Suite identifier
    pub suite_id: String,

    /// Suite status
    pub status: TestStatus,

    /// Suite execution time in milliseconds
    pub execution_time_ms: u64,

    /// Suite start time
    pub start_time: chrono::DateTime<chrono::Utc>,

    /// Suite end time
    pub end_time: chrono::DateTime<chrono::Utc>,

    /// Test results
    pub test_results: Vec<TestResult>,

    /// Suite summary
    pub summary: TestSummary,

    /// Suite metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Test summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    /// Total tests
    pub total_tests: u32,

    /// Passed tests
    pub passed_tests: u32,

    /// Failed tests
    pub failed_tests: u32,

    /// Skipped tests
    pub skipped_tests: u32,

    /// Error tests
    pub error_tests: u32,

    /// Timeout tests
    pub timeout_tests: u32,

    /// Success rate
    pub success_rate: f64,

    /// Average execution time
    pub average_execution_time_ms: f64,
}

/// Coverage report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    /// Overall coverage percentage
    pub overall_coverage: f64,

    /// Line coverage
    pub line_coverage: f64,

    /// Branch coverage
    pub branch_coverage: f64,

    /// Function coverage
    pub function_coverage: f64,

    /// File coverage details
    pub file_coverage: HashMap<String, FileCoverage>,

    /// Report timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// File coverage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCoverage {
    /// File path
    pub file_path: String,

    /// Line coverage
    pub line_coverage: f64,

    /// Branch coverage
    pub branch_coverage: f64,

    /// Function coverage
    pub function_coverage: f64,

    /// Covered lines
    pub covered_lines: Vec<u32>,

    /// Total lines
    pub total_lines: u32,

    /// Uncovered lines
    pub uncovered_lines: Vec<u32>,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Total execution time in milliseconds
    pub total_execution_time_ms: u64,

    /// Average test execution time in milliseconds
    pub average_test_execution_time_ms: f64,

    /// Slowest test execution time in milliseconds
    pub slowest_test_execution_time_ms: u64,

    /// Fastest test execution time in milliseconds
    pub fastest_test_execution_time_ms: u64,

    /// Memory usage in bytes
    pub memory_usage_bytes: u64,

    /// CPU usage percentage
    pub cpu_usage_percentage: f64,

    /// Test throughput (tests per second)
    pub test_throughput: f64,

    /// Metrics timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Unified testing framework implementation
pub struct UnifiedTestingFramework {
    /// Test configuration
    #[allow(dead_code)]
    config: TestConfig,

    /// Test environments
    #[allow(dead_code)]
    environments: HashMap<String, TestEnvironment>,

    /// Test results history
    #[allow(dead_code)]
    test_results_history: Vec<TestResult>,

    /// Performance metrics
    performance_metrics: PerformanceMetrics,
}

impl UnifiedTestingFramework {
    /// Create a new unified testing framework
    pub fn new(config: TestConfig) -> Self {
        Self {
            config,
            environments: HashMap::new(),
            test_results_history: Vec::new(),
            performance_metrics: PerformanceMetrics {
                total_execution_time_ms: 0,
                average_test_execution_time_ms: 0.0,
                slowest_test_execution_time_ms: 0,
                fastest_test_execution_time_ms: u64::MAX,
                memory_usage_bytes: 0,
                cpu_usage_percentage: 0.0,
                test_throughput: 0.0,
                timestamp: chrono::Utc::now(),
            },
        }
    }

    /// Update performance metrics
    #[allow(dead_code)]
    fn update_performance_metrics(&mut self, test_result: &TestResult) {
        self.performance_metrics.total_execution_time_ms += test_result.execution_time_ms;

        if test_result.execution_time_ms > self.performance_metrics.slowest_test_execution_time_ms {
            self.performance_metrics.slowest_test_execution_time_ms = test_result.execution_time_ms;
        }

        if test_result.execution_time_ms < self.performance_metrics.fastest_test_execution_time_ms {
            self.performance_metrics.fastest_test_execution_time_ms = test_result.execution_time_ms;
        }

        let total_tests = self.test_results_history.len() as f64;
        if total_tests > 0.0 {
            self.performance_metrics.average_test_execution_time_ms =
                self.performance_metrics.total_execution_time_ms as f64 / total_tests;
        }

        self.performance_metrics.timestamp = chrono::Utc::now();
    }
}

#[async_trait]
impl TestingFramework for UnifiedTestingFramework {
    async fn setup(&self, config: &TestConfig) -> Result<TestEnvironment> {
        let env_id = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string();

        info!(
            env_id = %env_id,
            environment_type = ?config.environment_type,
            "Setting up test environment"
        );

        let environment = TestEnvironment {
            id: env_id.clone(),
            environment_type: config.environment_type.clone(),
            state: EnvironmentState::Initializing,
            resources: HashMap::new(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };

        info!(
            env_id = %env_id,
            "Test environment setup completed"
        );

        Ok(environment)
    }

    async fn teardown(&self, env: &TestEnvironment) -> Result<()> {
        info!(
            env_id = %env.id,
            "Tearing down test environment"
        );

        // Clean up environment resources
        for (resource_key, resource_value) in &env.resources {
            debug!(
                env_id = %env.id,
                resource_key = %resource_key,
                "Cleaning up resource"
            );

            // Handle different resource types
            match resource_value {
                serde_json::Value::String(path)
                    if resource_key.contains("file") || resource_key.contains("dir") =>
                {
                    // Clean up temporary files/directories
                    if let Ok(path) = std::path::PathBuf::from(path).canonicalize() {
                        if path.exists() {
                            if path.is_file() {
                                if let Err(e) = tokio::fs::remove_file(&path).await {
                                    warn!(
                                        env_id = %env.id,
                                        resource = %resource_key,
                                        path = %path.display(),
                                        error = %e,
                                        "Failed to remove temporary file"
                                    );
                                }
                            } else if path.is_dir() {
                                if let Err(e) = tokio::fs::remove_dir_all(&path).await {
                                    warn!(
                                        env_id = %env.id,
                                        resource = %resource_key,
                                        path = %path.display(),
                                        error = %e,
                                        "Failed to remove temporary directory"
                                    );
                                }
                            }
                        }
                    }
                }
                serde_json::Value::Object(service_config) if resource_key.contains("service") => {
                    // Stop background services
                    if let Some(serde_json::Value::String(pid_str)) = service_config.get("pid") {
                        if let Ok(pid) = pid_str.parse::<i32>() {
                            // Attempt to terminate process
                            let _ = tokio::process::Command::new("kill")
                                .arg("-TERM")
                                .arg(pid.to_string())
                                .status()
                                .await;
                        }
                    }
                }
                _ => {
                    // Generic cleanup - just log
                    debug!(
                        env_id = %env.id,
                        resource_key = %resource_key,
                        "Generic resource cleanup (no specific handler)"
                    );
                }
            }
        }

        // Update environment state
        // Note: Since env is &TestEnvironment, we can't modify it directly
        // In a real implementation, this would update a mutable reference or database

        info!(
            env_id = %env.id,
            resources_cleaned = %env.resources.len(),
            "Test environment teardown completed"
        );

        Ok(())
    }

    async fn run_test(&self, test_case: &TestCase) -> Result<TestResult> {
        let start_time = chrono::Utc::now();
        let start_instant = std::time::Instant::now();

        info!(
            test_case_id = %test_case.id,
            test_name = %test_case.name,
            "Running test case"
        );

        let mut test_result = TestResult {
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

        // Run test steps
        for step in &test_case.steps {
            let step_result = self.run_test_step(step).await?;
            test_result.step_results.push(step_result);
        }

        // Run assertions
        for assertion in &test_case.assertions {
            let assertion_result = self.run_assertion(assertion).await?;
            test_result.assertion_results.push(assertion_result);
        }

        let end_time = chrono::Utc::now();
        let execution_time = start_instant.elapsed();

        test_result.end_time = end_time;
        test_result.execution_time_ms = execution_time.as_millis() as u64;

        // Determine final status
        if test_result
            .assertion_results
            .iter()
            .any(|r| r.status == TestStatus::Failed)
        {
            test_result.status = TestStatus::Failed;
        }

        info!(
            test_case_id = %test_case.id,
            status = ?test_result.status,
            execution_time_ms = test_result.execution_time_ms,
            "Test case completed"
        );

        Ok(test_result)
    }

    async fn run_suite(&self, suite: &TestSuite) -> Result<TestSuiteResult> {
        let start_time = chrono::Utc::now();
        let start_instant = std::time::Instant::now();

        info!(
            suite_id = %suite.id,
            suite_name = %suite.name,
            test_count = suite.test_cases.len(),
            "Running test suite"
        );

        let mut test_results = Vec::new();

        // Run test cases
        for test_case in &suite.test_cases {
            let test_result = self.run_test(test_case).await?;
            test_results.push(test_result);
        }

        let end_time = chrono::Utc::now();
        let execution_time = start_instant.elapsed();

        // Calculate summary
        let summary = TestSummary {
            total_tests: test_results.len() as u32,
            passed_tests: test_results
                .iter()
                .filter(|r| r.status == TestStatus::Passed)
                .count() as u32,
            failed_tests: test_results
                .iter()
                .filter(|r| r.status == TestStatus::Failed)
                .count() as u32,
            skipped_tests: test_results
                .iter()
                .filter(|r| r.status == TestStatus::Skipped)
                .count() as u32,
            error_tests: test_results
                .iter()
                .filter(|r| r.status == TestStatus::Error)
                .count() as u32,
            timeout_tests: test_results
                .iter()
                .filter(|r| r.status == TestStatus::Timeout)
                .count() as u32,
            success_rate: if test_results.is_empty() {
                0.0
            } else {
                test_results
                    .iter()
                    .filter(|r| r.status == TestStatus::Passed)
                    .count() as f64
                    / test_results.len() as f64
            },
            average_execution_time_ms: if test_results.is_empty() {
                0.0
            } else {
                test_results
                    .iter()
                    .map(|r| r.execution_time_ms)
                    .sum::<u64>() as f64
                    / test_results.len() as f64
            },
        };

        let suite_result = TestSuiteResult {
            suite_id: suite.id.clone(),
            status: if summary.failed_tests > 0 {
                TestStatus::Failed
            } else {
                TestStatus::Passed
            },
            execution_time_ms: execution_time.as_millis() as u64,
            start_time,
            end_time,
            test_results,
            summary,
            metadata: HashMap::new(),
        };

        info!(
            suite_id = %suite.id,
            status = ?suite_result.status,
            execution_time_ms = suite_result.execution_time_ms,
            success_rate = suite_result.summary.success_rate,
            "Test suite completed"
        );

        Ok(suite_result)
    }

    async fn get_coverage_report(&self) -> Result<CoverageReport> {
        let mut file_coverage = HashMap::new();
        let mut total_lines = 0u64;
        let mut covered_lines = 0u64;
        let mut total_functions = 0u64;
        let mut covered_functions = 0u64;

        // Try to read coverage data from common locations
        let coverage_sources = vec![
            "target/tarpaulin/lcov.info",
            "target/coverage/lcov.info",
            "lcov.info",
        ];

        for coverage_file in coverage_sources {
            if let Ok(content) = tokio::fs::read_to_string(coverage_file).await {
                debug!("Reading coverage data from {}", coverage_file);
                self.parse_lcov_coverage(
                    &content,
                    &mut file_coverage,
                    &mut total_lines,
                    &mut covered_lines,
                    &mut total_functions,
                    &mut covered_functions,
                )
                .await?;
                break; // Use first successful source
            }
        }

        // If no coverage data found, generate basic estimates from test execution
        if file_coverage.is_empty() {
            debug!("No coverage data found, generating basic estimates");
            self.generate_basic_coverage_estimates(
                &mut file_coverage,
                &mut total_lines,
                &mut covered_lines,
                &mut total_functions,
                &mut covered_functions,
            )
            .await?;
        }

        // Calculate percentages
        let line_coverage = if total_lines > 0 {
            (covered_lines as f64 / total_lines as f64) * 100.0
        } else {
            0.0
        };

        let function_coverage = if total_functions > 0 {
            (covered_functions as f64 / total_functions as f64) * 100.0
        } else {
            0.0
        };

        let overall_coverage = (line_coverage + function_coverage) / 2.0;
        let branch_coverage = line_coverage * 0.8; // Estimate branch coverage

        Ok(CoverageReport {
            overall_coverage,
            line_coverage,
            branch_coverage,
            function_coverage,
            file_coverage,
            timestamp: chrono::Utc::now(),
        })
    }

    async fn get_performance_metrics(&self) -> Result<PerformanceMetrics> {
        Ok(self.performance_metrics.clone())
    }
}

// Helper methods for UnifiedTestingFramework (not part of TestingFramework trait)
impl UnifiedTestingFramework {
    async fn parse_lcov_coverage(
        &self,
        content: &str,
        file_coverage: &mut HashMap<String, FileCoverage>,
        total_lines: &mut u64,
        covered_lines: &mut u64,
        total_functions: &mut u64,
        covered_functions: &mut u64,
    ) -> Result<()> {
        let mut current_file = String::new();
        let mut file_lines = 0u64;
        let mut file_covered_lines = 0u64;
        let mut file_functions = 0u64;
        let mut file_covered_functions = 0u64;
        let mut covered_line_numbers = Vec::new();

        for line in content.lines() {
            if let Some(stripped) = line.strip_prefix("SF:") {
                // Start of file
                if !current_file.is_empty() {
                    self.add_file_coverage(
                        file_coverage,
                        &current_file,
                        file_lines,
                        file_covered_lines,
                        file_functions,
                        file_covered_functions,
                        &covered_line_numbers,
                    );
                }
                current_file = stripped.to_string();
                file_lines = 0;
                file_covered_lines = 0;
                file_functions = 0;
                file_covered_functions = 0;
                covered_line_numbers.clear();
            } else if let Some(da_stripped) = line.strip_prefix("DA:") {
                // Line coverage data: DA:<line>,<hits>
                if let Some(comma_pos) = da_stripped.find(',') {
                    if let Ok(line_num) = da_stripped[..comma_pos].parse::<u32>() {
                        if let Ok(hits) = da_stripped[comma_pos + 1..].parse::<u32>() {
                            file_lines += 1;
                            *total_lines += 1;
                            if hits > 0 {
                                file_covered_lines += 1;
                                *covered_lines += 1;
                                covered_line_numbers.push(line_num);
                            }
                        }
                    }
                }
            } else if line.starts_with("FN:") {
                // Function definition
                file_functions += 1;
                *total_functions += 1;
            } else if let Some(fnda_stripped) = line.strip_prefix("FNDA:") {
                // Function coverage: FNDA:<hits>,<name>
                if let Some(comma_pos) = fnda_stripped.find(',') {
                    if let Ok(hits) = fnda_stripped[..comma_pos].parse::<u32>() {
                        if hits > 0 {
                            file_covered_functions += 1;
                            *covered_functions += 1;
                        }
                    }
                }
            }
        }

        // Add final file
        if !current_file.is_empty() {
            self.add_file_coverage(
                file_coverage,
                &current_file,
                file_lines,
                file_covered_lines,
                file_functions,
                file_covered_functions,
                &covered_line_numbers,
            );
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn add_file_coverage(
        &self,
        file_coverage: &mut HashMap<String, FileCoverage>,
        file_path: &str,
        lines: u64,
        covered_lines: u64,
        functions: u64,
        covered_functions: u64,
        covered_line_numbers: &[u32],
    ) {
        let line_coverage = if lines > 0 {
            (covered_lines as f64 / lines as f64) * 100.0
        } else {
            0.0
        };

        let function_coverage = if functions > 0 {
            (covered_functions as f64 / functions as f64) * 100.0
        } else {
            0.0
        };

        let branch_coverage = line_coverage * 0.8; // Estimate

        file_coverage.insert(
            file_path.to_string(),
            FileCoverage {
                file_path: file_path.to_string(),
                line_coverage,
                branch_coverage,
                function_coverage,
                covered_lines: covered_line_numbers.to_vec(),
                total_lines: lines as u32,
                uncovered_lines: Vec::new(), // Could be calculated from covered_lines
            },
        );
    }

    async fn generate_basic_coverage_estimates(
        &self,
        file_coverage: &mut HashMap<String, FileCoverage>,
        total_lines: &mut u64,
        covered_lines: &mut u64,
        total_functions: &mut u64,
        covered_functions: &mut u64,
    ) -> Result<()> {
        // Generate basic estimates based on codebase analysis
        // This is a fallback when no real coverage data is available

        let workspace_crates = std::fs::read_dir("crates")?;
        for entry in workspace_crates {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let _crate_name = entry.file_name().to_string_lossy().to_string();
                let src_dir = entry.path().join("src");

                if src_dir.exists() {
                    let rs_files = walkdir::WalkDir::new(&src_dir)
                        .into_iter()
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().extension() == Some(std::ffi::OsStr::new("rs")));

                    for file_entry in rs_files {
                        let file_path = file_entry.path();
                        if let Ok(content) = std::fs::read_to_string(file_path) {
                            let line_count = content.lines().count() as u64;
                            let function_count = content.matches("fn ").count() as u64;

                            // Estimate coverage based on test file presence
                            let test_file = file_path
                                .with_extension("")
                                .to_string_lossy()
                                .replace("/src/", "/tests/")
                                + "_test.rs";
                            let has_tests = std::path::Path::new(&test_file).exists();

                            let estimated_coverage = if has_tests { 0.75 } else { 0.3 };
                            let covered_line_count =
                                (line_count as f64 * estimated_coverage) as u64;
                            let covered_function_count =
                                (function_count as f64 * estimated_coverage) as u64;

                            *total_lines += line_count;
                            *covered_lines += covered_line_count;
                            *total_functions += function_count;
                            *covered_functions += covered_function_count;

                            file_coverage.insert(
                                file_path.to_string_lossy().to_string(),
                                FileCoverage {
                                    file_path: file_path.to_string_lossy().to_string(),
                                    line_coverage: estimated_coverage * 100.0,
                                    branch_coverage: estimated_coverage * 80.0,
                                    function_coverage: estimated_coverage * 100.0,
                                    covered_lines: Vec::new(), // Not calculated in basic mode
                                    total_lines: line_count as u32,
                                    uncovered_lines: Vec::new(),
                                },
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn execute_command_step(
        &self,
        step: &TestStep,
        command: &str,
        args: &[String],
        result: &mut TestStepResult,
    ) -> Result<()> {
        debug!(
            step_id = %step.id,
            command = %command,
            args = ?args,
            "Executing command step"
        );

        let output = tokio::process::Command::new(command)
            .args(args)
            .output()
            .await
            .map_err(|e| {
                result.status = TestStatus::Error;
                result.error = Some(format!("Failed to execute command: {}", e));
                e
            })?;

        if output.status.success() {
            result.status = TestStatus::Passed;
            result.output = Some(String::from_utf8_lossy(&output.stdout).to_string());
        } else {
            result.status = TestStatus::Failed;
            result.error = Some(String::from_utf8_lossy(&output.stderr).to_string());
        }

        Ok(())
    }

    async fn execute_api_call_step(
        &self,
        step: &TestStep,
        method: &str,
        url: &str,
        body: Option<&serde_json::Value>,
        result: &mut TestStepResult,
    ) -> Result<()> {
        debug!(
            step_id = %step.id,
            method = %method,
            url = %url,
            "Executing API call step"
        );

        // Basic HTTP client implementation
        // In a real implementation, this would use reqwest or similar
        let client = reqwest::Client::new();
        let mut request = match method.to_uppercase().as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            _ => {
                result.status = TestStatus::Error;
                result.error = Some(format!("Unsupported HTTP method: {}", method));
                return Ok(());
            }
        };

        if let Some(body_data) = body {
            request = request.json(body_data);
        }

        match request.send().await {
            Ok(response) => {
                if response.status().is_success() {
                    result.status = TestStatus::Passed;
                    result.output = Some(format!("HTTP {} - Success", response.status()));
                } else {
                    result.status = TestStatus::Failed;
                    result.error = Some(format!(
                        "HTTP {} - {}",
                        response.status(),
                        response
                            .status()
                            .canonical_reason()
                            .unwrap_or("Unknown error")
                    ));
                }
            }
            Err(e) => {
                result.status = TestStatus::Error;
                result.error = Some(format!("API call failed: {}", e));
            }
        }

        Ok(())
    }

    async fn execute_database_step(
        &self,
        step: &TestStep,
        operation: &str,
        query: &str,
        params: &[serde_json::Value],
        result: &mut TestStepResult,
    ) -> Result<()> {
        debug!(
            step_id = %step.id,
            operation = %operation,
            query = %query,
            "Executing database step"
        );

        // Get database path from step parameters or use default test database
        let db_path = step
            .parameters
            .get("db_path")
            .and_then(|v| v.as_str())
            .unwrap_or(":memory:");

        // Execute database operation using SQLite
        match sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite:{}", db_path))
            .await
        {
            Ok(pool) => {
                match operation.to_lowercase().as_str() {
                    "select" => {
                        // Execute SELECT query and return results
                        let mut query_builder = sqlx::query(query);
                        for param in params {
                            query_builder = match param {
                                serde_json::Value::String(s) => query_builder.bind(s.clone()),
                                serde_json::Value::Number(n) => {
                                    if let Some(i) = n.as_i64() {
                                        query_builder.bind(i)
                                    } else if let Some(f) = n.as_f64() {
                                        query_builder.bind(f)
                                    } else {
                                        query_builder
                                    }
                                }
                                serde_json::Value::Bool(b) => query_builder.bind(*b),
                                serde_json::Value::Null => {
                                    query_builder.bind(Option::<String>::None)
                                }
                                _ => query_builder.bind(param.to_string()),
                            };
                        }

                        match query_builder.fetch_all(&pool).await {
                            Ok(rows) => {
                                result.status = TestStatus::Passed;
                                result.output =
                                    Some(format!("SELECT returned {} rows", rows.len()));
                            }
                            Err(e) => {
                                result.status = TestStatus::Failed;
                                result.error = Some(format!("SELECT failed: {}", e));
                            }
                        }
                    }
                    "insert" | "update" | "delete" => {
                        // Execute mutation query
                        let mut query_builder = sqlx::query(query);
                        for param in params {
                            query_builder = match param {
                                serde_json::Value::String(s) => query_builder.bind(s.clone()),
                                serde_json::Value::Number(n) => {
                                    if let Some(i) = n.as_i64() {
                                        query_builder.bind(i)
                                    } else if let Some(f) = n.as_f64() {
                                        query_builder.bind(f)
                                    } else {
                                        query_builder
                                    }
                                }
                                serde_json::Value::Bool(b) => query_builder.bind(*b),
                                serde_json::Value::Null => {
                                    query_builder.bind(Option::<String>::None)
                                }
                                _ => query_builder.bind(param.to_string()),
                            };
                        }

                        match query_builder.execute(&pool).await {
                            Ok(query_result) => {
                                result.status = TestStatus::Passed;
                                result.output = Some(format!(
                                    "{} affected {} rows",
                                    operation.to_uppercase(),
                                    query_result.rows_affected()
                                ));
                            }
                            Err(e) => {
                                result.status = TestStatus::Failed;
                                result.error =
                                    Some(format!("{} failed: {}", operation.to_uppercase(), e));
                            }
                        }
                    }
                    "execute" => {
                        // Raw execution (for CREATE TABLE, etc.)
                        match sqlx::query(query).execute(&pool).await {
                            Ok(_) => {
                                result.status = TestStatus::Passed;
                                result.output = Some("Statement executed successfully".to_string());
                            }
                            Err(e) => {
                                result.status = TestStatus::Failed;
                                result.error = Some(format!("Execution failed: {}", e));
                            }
                        }
                    }
                    _ => {
                        result.status = TestStatus::Error;
                        result.error =
                            Some(format!("Unsupported database operation: {}", operation));
                    }
                }

                // Close pool
                pool.close().await;
            }
            Err(e) => {
                result.status = TestStatus::Error;
                result.error = Some(format!("Failed to connect to database: {}", e));
            }
        }

        Ok(())
    }

    async fn execute_file_step(
        &self,
        step: &TestStep,
        operation: &str,
        path: &str,
        content: Option<&String>,
        result: &mut TestStepResult,
    ) -> Result<()> {
        debug!(
            step_id = %step.id,
            operation = %operation,
            path = %path,
            "Executing file step"
        );

        match operation.to_lowercase().as_str() {
            "create" => {
                if let Some(content) = content {
                    match tokio::fs::write(path, content).await {
                        Ok(_) => {
                            result.status = TestStatus::Passed;
                            result.output = Some(format!("File created: {}", path));
                        }
                        Err(e) => {
                            result.status = TestStatus::Error;
                            result.error = Some(format!("Failed to create file: {}", e));
                        }
                    }
                } else {
                    result.status = TestStatus::Error;
                    result.error = Some("File create operation requires content".to_string());
                }
            }
            "read" => match tokio::fs::read_to_string(path).await {
                Ok(content) => {
                    result.status = TestStatus::Passed;
                    result.output = Some(content);
                }
                Err(e) => {
                    result.status = TestStatus::Error;
                    result.error = Some(format!("Failed to read file: {}", e));
                }
            },
            "delete" => match tokio::fs::remove_file(path).await {
                Ok(_) => {
                    result.status = TestStatus::Passed;
                    result.output = Some(format!("File deleted: {}", path));
                }
                Err(e) => {
                    result.status = TestStatus::Error;
                    result.error = Some(format!("Failed to delete file: {}", e));
                }
            },
            _ => {
                result.status = TestStatus::Error;
                result.error = Some(format!("Unsupported file operation: {}", operation));
            }
        }

        Ok(())
    }

    async fn execute_wait_step(
        &self,
        step: &TestStep,
        duration_ms: u64,
        result: &mut TestStepResult,
    ) -> Result<()> {
        debug!(
            step_id = %step.id,
            duration_ms = duration_ms,
            "Executing wait step"
        );

        tokio::time::sleep(tokio::time::Duration::from_millis(duration_ms)).await;
        result.status = TestStatus::Passed;
        result.output = Some(format!("Waited {}ms", duration_ms));

        Ok(())
    }

    async fn execute_assertion_step(
        &self,
        step: &TestStep,
        condition: &str,
        expected: &serde_json::Value,
        result: &mut TestStepResult,
    ) -> Result<()> {
        debug!(
            step_id = %step.id,
            condition = %condition,
            expected = ?expected,
            "Executing assertion step"
        );

        // Get actual value from step parameters
        let actual = step.parameters.get("actual");

        match condition {
            "equals" => {
                if let Some(actual_val) = actual {
                    if actual_val == expected {
                        result.status = TestStatus::Passed;
                        result.output = Some(format!(
                            "Assertion passed: {:?} equals {:?}",
                            actual_val, expected
                        ));
                    } else {
                        result.status = TestStatus::Failed;
                        result.error = Some(format!(
                            "Assertion failed: expected {:?}, got {:?}",
                            expected, actual_val
                        ));
                    }
                } else {
                    result.status = TestStatus::Failed;
                    result.error =
                        Some("Missing 'actual' parameter for equals assertion".to_string());
                }
            }
            "not_equals" => {
                if let Some(actual_val) = actual {
                    if actual_val != expected {
                        result.status = TestStatus::Passed;
                        result.output = Some(format!(
                            "Assertion passed: {:?} not equals {:?}",
                            actual_val, expected
                        ));
                    } else {
                        result.status = TestStatus::Failed;
                        result.error = Some(format!(
                            "Assertion failed: values are equal: {:?}",
                            expected
                        ));
                    }
                } else {
                    result.status = TestStatus::Failed;
                    result.error =
                        Some("Missing 'actual' parameter for not_equals assertion".to_string());
                }
            }
            "contains" => {
                if let (
                    Some(serde_json::Value::String(haystack)),
                    serde_json::Value::String(needle),
                ) = (actual, expected)
                {
                    if haystack.contains(needle) {
                        result.status = TestStatus::Passed;
                        result.output = Some(format!(
                            "Assertion passed: '{}' contains '{}'",
                            haystack, needle
                        ));
                    } else {
                        result.status = TestStatus::Failed;
                        result.error = Some(format!(
                            "Assertion failed: '{}' does not contain '{}'",
                            haystack, needle
                        ));
                    }
                } else {
                    result.status = TestStatus::Failed;
                    result.error =
                        Some("Contains assertion requires string parameters".to_string());
                }
            }
            "exists" => {
                // Check if a file or key exists
                if let serde_json::Value::String(path) = expected {
                    if std::path::Path::new(path).exists() {
                        result.status = TestStatus::Passed;
                        result.output = Some(format!("Assertion passed: '{}' exists", path));
                    } else {
                        result.status = TestStatus::Failed;
                        result.error = Some(format!("Assertion failed: '{}' does not exist", path));
                    }
                } else {
                    result.status = TestStatus::Failed;
                    result.error = Some("Exists assertion requires a string path".to_string());
                }
            }
            "greater_than" => {
                if let (Some(actual_val), Some(expected_num)) =
                    (actual.and_then(|v| v.as_f64()), expected.as_f64())
                {
                    if actual_val > expected_num {
                        result.status = TestStatus::Passed;
                        result.output = Some(format!(
                            "Assertion passed: {} > {}",
                            actual_val, expected_num
                        ));
                    } else {
                        result.status = TestStatus::Failed;
                        result.error = Some(format!(
                            "Assertion failed: {} is not greater than {}",
                            actual_val, expected_num
                        ));
                    }
                } else {
                    result.status = TestStatus::Failed;
                    result.error =
                        Some("Greater than assertion requires numeric parameters".to_string());
                }
            }
            "less_than" => {
                if let (Some(actual_val), Some(expected_num)) =
                    (actual.and_then(|v| v.as_f64()), expected.as_f64())
                {
                    if actual_val < expected_num {
                        result.status = TestStatus::Passed;
                        result.output = Some(format!(
                            "Assertion passed: {} < {}",
                            actual_val, expected_num
                        ));
                    } else {
                        result.status = TestStatus::Failed;
                        result.error = Some(format!(
                            "Assertion failed: {} is not less than {}",
                            actual_val, expected_num
                        ));
                    }
                } else {
                    result.status = TestStatus::Failed;
                    result.error =
                        Some("Less than assertion requires numeric parameters".to_string());
                }
            }
            "regex" => {
                if let (Some(serde_json::Value::String(text)), serde_json::Value::String(pattern)) =
                    (actual, expected)
                {
                    match regex::Regex::new(pattern) {
                        Ok(re) => {
                            if re.is_match(text) {
                                result.status = TestStatus::Passed;
                                result.output = Some(format!(
                                    "Assertion passed: text matches pattern '{}'",
                                    pattern
                                ));
                            } else {
                                result.status = TestStatus::Failed;
                                result.error = Some(format!(
                                    "Assertion failed: text does not match pattern '{}'",
                                    pattern
                                ));
                            }
                        }
                        Err(e) => {
                            result.status = TestStatus::Error;
                            result.error = Some(format!("Invalid regex pattern: {}", e));
                        }
                    }
                } else {
                    result.status = TestStatus::Failed;
                    result.error = Some("Regex assertion requires string parameters".to_string());
                }
            }
            "not_empty" => {
                if let Some(actual_val) = actual {
                    let is_empty = match actual_val {
                        serde_json::Value::String(s) => s.is_empty(),
                        serde_json::Value::Array(a) => a.is_empty(),
                        serde_json::Value::Object(o) => o.is_empty(),
                        serde_json::Value::Null => true,
                        _ => false,
                    };
                    if !is_empty {
                        result.status = TestStatus::Passed;
                        result.output = Some("Assertion passed: value is not empty".to_string());
                    } else {
                        result.status = TestStatus::Failed;
                        result.error = Some("Assertion failed: value is empty".to_string());
                    }
                } else {
                    result.status = TestStatus::Failed;
                    result.error =
                        Some("Missing 'actual' parameter for not_empty assertion".to_string());
                }
            }
            _ => {
                result.status = TestStatus::Error;
                result.error = Some(format!("Unsupported assertion condition: {}", condition));
            }
        }

        Ok(())
    }

    async fn execute_custom_step(
        &self,
        step: &TestStep,
        action_type: &str,
        parameters: &HashMap<String, serde_json::Value>,
        result: &mut TestStepResult,
    ) -> Result<()> {
        debug!(
            step_id = %step.id,
            action_type = %action_type,
            parameters = ?parameters,
            "Executing custom step"
        );

        // Extensible custom action handling
        match action_type {
            "sleep" => {
                // Sleep for a specified duration
                let duration_ms = parameters
                    .get("duration_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1000);
                tokio::time::sleep(tokio::time::Duration::from_millis(duration_ms)).await;
                result.status = TestStatus::Passed;
                result.output = Some(format!("Slept for {}ms", duration_ms));
            }
            "log" => {
                // Log a message
                let message = parameters
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Custom log message");
                let level = parameters
                    .get("level")
                    .and_then(|v| v.as_str())
                    .unwrap_or("info");

                match level {
                    "debug" => debug!(step_id = %step.id, "{}", message),
                    "info" => info!(step_id = %step.id, "{}", message),
                    "warn" => warn!(step_id = %step.id, "{}", message),
                    _ => info!(step_id = %step.id, "{}", message),
                }

                result.status = TestStatus::Passed;
                result.output = Some(format!("Logged: {}", message));
            }
            "set_env" => {
                // Set an environment variable for test context
                let key = parameters
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AosError::Config("Missing 'key' for set_env action".to_string())
                    })?;
                let value = parameters
                    .get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AosError::Config("Missing 'value' for set_env action".to_string())
                    })?;

                std::env::set_var(key, value);
                result.status = TestStatus::Passed;
                result.output = Some(format!("Set env var {}={}", key, value));
            }
            "get_env" => {
                // Get an environment variable
                let key = parameters
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AosError::Config("Missing 'key' for get_env action".to_string())
                    })?;

                match std::env::var(key) {
                    Ok(value) => {
                        result.status = TestStatus::Passed;
                        result.output = Some(value);
                    }
                    Err(_) => {
                        result.status = TestStatus::Failed;
                        result.error = Some(format!("Environment variable '{}' not found", key));
                    }
                }
            }
            "json_parse" => {
                // Parse and validate JSON
                let json_str =
                    parameters
                        .get("json")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            AosError::Config("Missing 'json' for json_parse action".to_string())
                        })?;

                match serde_json::from_str::<serde_json::Value>(json_str) {
                    Ok(parsed) => {
                        result.status = TestStatus::Passed;
                        result.output = Some(format!("JSON parsed successfully: {}", parsed));
                    }
                    Err(e) => {
                        result.status = TestStatus::Failed;
                        result.error = Some(format!("JSON parse error: {}", e));
                    }
                }
            }
            "timestamp" => {
                // Generate a timestamp
                let timestamp = chrono::Utc::now().to_rfc3339();
                result.status = TestStatus::Passed;
                result.output = Some(timestamp);
            }
            "uuid" => {
                // Generate a UUID
                let uuid = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string();
                result.status = TestStatus::Passed;
                result.output = Some(uuid);
            }
            "hash" => {
                // Generate a hash of input data
                let data = parameters
                    .get("data")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                data.hash(&mut hasher);
                let hash = hasher.finish();

                result.status = TestStatus::Passed;
                result.output = Some(format!("{:x}", hash));
            }
            _ => {
                // Unknown custom action type
                result.status = TestStatus::Error;
                result.error = Some(format!(
                    "Unknown custom action type: '{}'. Supported: sleep, log, set_env, get_env, json_parse, timestamp, uuid, hash",
                    action_type
                ));
            }
        }

        Ok(())
    }

    async fn execute_equals_assertion(
        &self,
        assertion: &TestAssertion,
        result: &mut AssertionResult,
    ) -> Result<()> {
        let expected = assertion.parameters.get("expected").ok_or_else(|| {
            AosError::Config("Missing 'expected' parameter for equals assertion".to_string())
        })?;
        let actual = assertion.parameters.get("actual").ok_or_else(|| {
            AosError::Config("Missing 'actual' parameter for equals assertion".to_string())
        })?;

        if expected == actual {
            result.status = TestStatus::Passed;
            result.message = Some("Values are equal".to_string());
        } else {
            result.status = TestStatus::Failed;
            result.message = Some(format!("Expected {:?}, got {:?}", expected, actual));
        }

        Ok(())
    }

    async fn execute_not_equals_assertion(
        &self,
        assertion: &TestAssertion,
        result: &mut AssertionResult,
    ) -> Result<()> {
        let expected = assertion.parameters.get("expected").ok_or_else(|| {
            AosError::Config("Missing 'expected' parameter for not_equals assertion".to_string())
        })?;
        let actual = assertion.parameters.get("actual").ok_or_else(|| {
            AosError::Config("Missing 'actual' parameter for not_equals assertion".to_string())
        })?;

        if expected != actual {
            result.status = TestStatus::Passed;
            result.message = Some("Values are not equal".to_string());
        } else {
            result.status = TestStatus::Failed;
            result.message = Some(format!("Values are equal: {:?}", expected));
        }

        Ok(())
    }

    async fn execute_greater_than_assertion(
        &self,
        assertion: &TestAssertion,
        result: &mut AssertionResult,
    ) -> Result<()> {
        let value = assertion
            .parameters
            .get("value")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| {
                AosError::Config(
                    "Missing or invalid 'value' parameter for greater_than assertion".to_string(),
                )
            })?;
        let threshold = assertion
            .parameters
            .get("threshold")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| {
                AosError::Config(
                    "Missing or invalid 'threshold' parameter for greater_than assertion"
                        .to_string(),
                )
            })?;

        if value > threshold {
            result.status = TestStatus::Passed;
            result.message = Some(format!("{} > {}", value, threshold));
        } else {
            result.status = TestStatus::Failed;
            result.message = Some(format!("{} is not greater than {}", value, threshold));
        }

        Ok(())
    }

    async fn execute_less_than_assertion(
        &self,
        assertion: &TestAssertion,
        result: &mut AssertionResult,
    ) -> Result<()> {
        let value = assertion
            .parameters
            .get("value")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| {
                AosError::Config(
                    "Missing or invalid 'value' parameter for less_than assertion".to_string(),
                )
            })?;
        let threshold = assertion
            .parameters
            .get("threshold")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| {
                AosError::Config(
                    "Missing or invalid 'threshold' parameter for less_than assertion".to_string(),
                )
            })?;

        if value < threshold {
            result.status = TestStatus::Passed;
            result.message = Some(format!("{} < {}", value, threshold));
        } else {
            result.status = TestStatus::Failed;
            result.message = Some(format!("{} is not less than {}", value, threshold));
        }

        Ok(())
    }

    async fn execute_contains_assertion(
        &self,
        assertion: &TestAssertion,
        result: &mut AssertionResult,
    ) -> Result<()> {
        let haystack = assertion
            .parameters
            .get("haystack")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AosError::Config(
                    "Missing or invalid 'haystack' parameter for contains assertion".to_string(),
                )
            })?;
        let needle = assertion
            .parameters
            .get("needle")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AosError::Config(
                    "Missing or invalid 'needle' parameter for contains assertion".to_string(),
                )
            })?;

        if haystack.contains(needle) {
            result.status = TestStatus::Passed;
            result.message = Some(format!("'{}' contains '{}'", haystack, needle));
        } else {
            result.status = TestStatus::Failed;
            result.message = Some(format!("'{}' does not contain '{}'", haystack, needle));
        }

        Ok(())
    }

    async fn execute_not_contains_assertion(
        &self,
        assertion: &TestAssertion,
        result: &mut AssertionResult,
    ) -> Result<()> {
        let haystack = assertion
            .parameters
            .get("haystack")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AosError::Config(
                    "Missing or invalid 'haystack' parameter for not_contains assertion"
                        .to_string(),
                )
            })?;
        let needle = assertion
            .parameters
            .get("needle")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AosError::Config(
                    "Missing or invalid 'needle' parameter for not_contains assertion".to_string(),
                )
            })?;

        if !haystack.contains(needle) {
            result.status = TestStatus::Passed;
            result.message = Some(format!("'{}' does not contain '{}'", haystack, needle));
        } else {
            result.status = TestStatus::Failed;
            result.message = Some(format!("'{}' contains '{}'", haystack, needle));
        }

        Ok(())
    }

    async fn execute_regex_assertion(
        &self,
        assertion: &TestAssertion,
        result: &mut AssertionResult,
    ) -> Result<()> {
        let pattern = assertion
            .parameters
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AosError::Config(
                    "Missing or invalid 'pattern' parameter for regex assertion".to_string(),
                )
            })?;
        let text = assertion
            .parameters
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AosError::Config(
                    "Missing or invalid 'text' parameter for regex assertion".to_string(),
                )
            })?;

        match regex::Regex::new(pattern) {
            Ok(re) => {
                if re.is_match(text) {
                    result.status = TestStatus::Passed;
                    result.message = Some(format!("Text matches regex pattern: {}", pattern));
                } else {
                    result.status = TestStatus::Failed;
                    result.message =
                        Some(format!("Text does not match regex pattern: {}", pattern));
                }
            }
            Err(e) => {
                result.status = TestStatus::Error;
                result.message = Some(format!("Invalid regex pattern: {}", e));
            }
        }

        Ok(())
    }

    async fn execute_file_exists_assertion(
        &self,
        assertion: &TestAssertion,
        result: &mut AssertionResult,
    ) -> Result<()> {
        let path = assertion
            .parameters
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AosError::Config(
                    "Missing or invalid 'path' parameter for file_exists assertion".to_string(),
                )
            })?;

        if std::path::Path::new(path).exists() {
            result.status = TestStatus::Passed;
            result.message = Some(format!("File exists: {}", path));
        } else {
            result.status = TestStatus::Failed;
            result.message = Some(format!("File does not exist: {}", path));
        }

        Ok(())
    }

    async fn execute_json_path_assertion(
        &self,
        assertion: &TestAssertion,
        result: &mut AssertionResult,
    ) -> Result<()> {
        // Get required parameters
        let json_data = assertion.parameters.get("json").ok_or_else(|| {
            AosError::Config("Missing 'json' parameter for JSONPath assertion".to_string())
        })?;

        let path = assertion
            .parameters
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AosError::Config(
                    "Missing or invalid 'path' parameter for JSONPath assertion".to_string(),
                )
            })?;

        let expected = assertion.parameters.get("expected");

        // Simple JSONPath implementation supporting basic paths like "$.key", "$.key.subkey", "$.array[0]"
        let extracted = self.extract_json_path(json_data, path);

        match (extracted, expected) {
            (Some(value), Some(expected_value)) => {
                // Compare with expected value
                if &value == expected_value {
                    result.status = TestStatus::Passed;
                    result.message = Some(format!(
                        "JSONPath '{}' matched expected value: {:?}",
                        path, value
                    ));
                } else {
                    result.status = TestStatus::Failed;
                    result.message = Some(format!(
                        "JSONPath '{}': expected {:?}, got {:?}",
                        path, expected_value, value
                    ));
                }
            }
            (Some(value), None) => {
                // Just check if path exists and return value
                result.status = TestStatus::Passed;
                result.message = Some(format!("JSONPath '{}' found: {:?}", path, value));
                result.details = Some(value);
            }
            (None, _) => {
                result.status = TestStatus::Failed;
                result.message = Some(format!("JSONPath '{}' not found in JSON data", path));
            }
        }

        Ok(())
    }

    /// Extract a value from JSON using a simple JSONPath expression
    fn extract_json_path(&self, json: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
        // Handle root reference
        let path = path
            .strip_prefix("$.")
            .unwrap_or(path.strip_prefix("$").unwrap_or(path));

        if path.is_empty() {
            return Some(json.clone());
        }

        let mut current = json.clone();

        for segment in path.split('.') {
            // Check for array index: key[0]
            if let Some(bracket_pos) = segment.find('[') {
                let key = &segment[..bracket_pos];
                let index_str = segment[bracket_pos + 1..].trim_end_matches(']');

                // First get the object by key
                if !key.is_empty() {
                    current = current.get(key)?.clone();
                }

                // Then get the array element by index
                if let Ok(index) = index_str.parse::<usize>() {
                    current = current.get(index)?.clone();
                } else {
                    return None;
                }
            } else {
                // Simple key access
                current = current.get(segment)?.clone();
            }
        }

        Some(current)
    }
}

impl UnifiedTestingFramework {
    /// Run a test step
    async fn run_test_step(&self, step: &TestStep) -> Result<StepResult> {
        let start_instant = std::time::Instant::now();

        debug!(
            step_id = %step.id,
            step_name = %step.name,
            "Running test step"
        );

        let mut step_result = StepResult {
            step_id: step.id.clone(),
            status: TestStatus::Passed,
            output: None,
            error: None,
            execution_time_ms: 0,
        };

        // Execute the step based on action type
        match &step.action {
            TestAction::ExecuteCommand { command, args } => {
                self.execute_command_step(step, command, args, &mut step_result)
                    .await?;
            }
            TestAction::ApiCall { method, url, body } => {
                self.execute_api_call_step(step, method, url, body.as_ref(), &mut step_result)
                    .await?;
            }
            TestAction::DatabaseOperation {
                operation,
                query,
                params,
            } => {
                self.execute_database_step(step, operation, query, params, &mut step_result)
                    .await?;
            }
            TestAction::FileOperation {
                operation,
                path,
                content,
            } => {
                self.execute_file_step(step, operation, path, content.as_ref(), &mut step_result)
                    .await?;
            }
            TestAction::Wait { duration_ms } => {
                self.execute_wait_step(step, *duration_ms, &mut step_result)
                    .await?;
            }
            TestAction::Assert {
                condition,
                expected,
            } => {
                self.execute_assertion_step(step, condition, expected, &mut step_result)
                    .await?;
            }
            TestAction::Custom { action_type, data } => {
                // Convert data to HashMap for execute_custom_step
                let parameters: HashMap<String, serde_json::Value> =
                    if let serde_json::Value::Object(map) = data {
                        map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                    } else {
                        HashMap::new()
                    };
                self.execute_custom_step(step, action_type, &parameters, &mut step_result)
                    .await?;
            }
            TestAction::NetworkOperation {
                operation,
                host,
                port,
            } => {
                debug!(
                    step_id = %step.id,
                    operation = %operation,
                    host = %host,
                    port = port,
                    "Executing network operation"
                );

                match operation.as_str() {
                    "ping" | "connect" => {
                        // Test TCP connection
                        let addr = format!("{}:{}", host, port);
                        match tokio::time::timeout(
                            tokio::time::Duration::from_secs(5),
                            tokio::net::TcpStream::connect(&addr),
                        )
                        .await
                        {
                            Ok(Ok(_)) => {
                                step_result.status = TestStatus::Passed;
                                step_result.output =
                                    Some(format!("Successfully connected to {}", addr));
                            }
                            Ok(Err(e)) => {
                                step_result.status = TestStatus::Failed;
                                step_result.error =
                                    Some(format!("Connection failed to {}: {}", addr, e));
                            }
                            Err(_) => {
                                step_result.status = TestStatus::Timeout;
                                step_result.error = Some(format!("Connection timeout to {}", addr));
                            }
                        }
                    }
                    "dns" | "resolve" => {
                        // DNS resolution
                        match tokio::net::lookup_host(format!("{}:{}", host, port)).await {
                            Ok(addrs) => {
                                let addrs: Vec<_> = addrs.collect();
                                step_result.status = TestStatus::Passed;
                                step_result.output =
                                    Some(format!("Resolved {} to {} addresses", host, addrs.len()));
                            }
                            Err(e) => {
                                step_result.status = TestStatus::Failed;
                                step_result.error =
                                    Some(format!("DNS resolution failed for {}: {}", host, e));
                            }
                        }
                    }
                    "port_check" => {
                        // Check if port is open
                        let addr = format!("{}:{}", host, port);
                        match tokio::time::timeout(
                            tokio::time::Duration::from_secs(2),
                            tokio::net::TcpStream::connect(&addr),
                        )
                        .await
                        {
                            Ok(Ok(_)) => {
                                step_result.status = TestStatus::Passed;
                                step_result.output =
                                    Some(format!("Port {} is open on {}", port, host));
                            }
                            _ => {
                                step_result.status = TestStatus::Failed;
                                step_result.error =
                                    Some(format!("Port {} is closed on {}", port, host));
                            }
                        }
                    }
                    _ => {
                        step_result.status = TestStatus::Error;
                        step_result.error = Some(format!(
                            "Unsupported network operation: '{}'. Supported: ping, connect, dns, resolve, port_check",
                            operation
                        ));
                    }
                }
            }
        }

        let execution_time = start_instant.elapsed();
        step_result.execution_time_ms = execution_time.as_millis() as u64;

        debug!(
            step_id = %step.id,
            status = ?step_result.status,
            execution_time_ms = step_result.execution_time_ms,
            "Test step completed"
        );

        Ok(step_result)
    }

    /// Run an assertion
    async fn run_assertion(&self, assertion: &TestAssertion) -> Result<AssertionResult> {
        debug!(
            assertion_id = %assertion.id,
            assertion_name = %assertion.name,
            "Running test assertion"
        );

        let mut assertion_result = AssertionResult {
            assertion_id: assertion.id.clone(),
            status: TestStatus::Passed,
            message: None,
            details: None,
        };

        // Execute assertion based on type
        match &assertion.assertion_type {
            AssertionType::Equals => {
                self.execute_equals_assertion(assertion, &mut assertion_result)
                    .await?;
            }
            AssertionType::NotEquals => {
                self.execute_not_equals_assertion(assertion, &mut assertion_result)
                    .await?;
            }
            AssertionType::GreaterThan => {
                self.execute_greater_than_assertion(assertion, &mut assertion_result)
                    .await?;
            }
            AssertionType::LessThan => {
                self.execute_less_than_assertion(assertion, &mut assertion_result)
                    .await?;
            }
            AssertionType::Contains => {
                self.execute_contains_assertion(assertion, &mut assertion_result)
                    .await?;
            }
            AssertionType::NotContains => {
                self.execute_not_contains_assertion(assertion, &mut assertion_result)
                    .await?;
            }
            AssertionType::RegexMatch => {
                self.execute_regex_assertion(assertion, &mut assertion_result)
                    .await?;
            }
            AssertionType::FileExists => {
                self.execute_file_exists_assertion(assertion, &mut assertion_result)
                    .await?;
            }
            AssertionType::FileNotExists => {
                // Similar to FileExists but inverted
                let path = assertion
                    .parameters
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AosError::Config(
                            "Missing or invalid 'path' parameter for file_not_exists assertion"
                                .to_string(),
                        )
                    })?;

                if !std::path::Path::new(path).exists() {
                    assertion_result.status = TestStatus::Passed;
                    assertion_result.message = Some(format!("File does not exist: {}", path));
                } else {
                    assertion_result.status = TestStatus::Failed;
                    assertion_result.message = Some(format!("File exists: {}", path));
                }
            }
            AssertionType::DatabaseRecordExists => {
                // Database record existence check
                let db_path = assertion
                    .parameters
                    .get("db_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(":memory:");
                let table = assertion
                    .parameters
                    .get("table")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AosError::Config(
                            "Missing 'table' parameter for DatabaseRecordExists assertion"
                                .to_string(),
                        )
                    })?;
                let condition = assertion
                    .parameters
                    .get("condition")
                    .and_then(|v| v.as_str())
                    .unwrap_or("1=1");

                // Connect and query
                match sqlx::sqlite::SqlitePoolOptions::new()
                    .max_connections(1)
                    .connect(&format!("sqlite:{}", db_path))
                    .await
                {
                    Ok(pool) => {
                        let query =
                            format!("SELECT COUNT(*) as cnt FROM {} WHERE {}", table, condition);
                        match sqlx::query_scalar::<_, i64>(&query).fetch_one(&pool).await {
                            Ok(count) => {
                                if count > 0 {
                                    assertion_result.status = TestStatus::Passed;
                                    assertion_result.message = Some(format!(
                                        "Found {} record(s) in {} where {}",
                                        count, table, condition
                                    ));
                                } else {
                                    assertion_result.status = TestStatus::Failed;
                                    assertion_result.message = Some(format!(
                                        "No records found in {} where {}",
                                        table, condition
                                    ));
                                }
                            }
                            Err(e) => {
                                assertion_result.status = TestStatus::Error;
                                assertion_result.message =
                                    Some(format!("Database query failed: {}", e));
                            }
                        }
                        pool.close().await;
                    }
                    Err(e) => {
                        assertion_result.status = TestStatus::Error;
                        assertion_result.message =
                            Some(format!("Database connection failed: {}", e));
                    }
                }
            }
            AssertionType::ApiResponse => {
                // API response validation
                let url = assertion
                    .parameters
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AosError::Config(
                            "Missing 'url' parameter for ApiResponse assertion".to_string(),
                        )
                    })?;
                let method = assertion
                    .parameters
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("GET");
                let expected_status = assertion
                    .parameters
                    .get("expected_status")
                    .and_then(|v| v.as_u64())
                    .map(|s| s as u16);
                let expected_body_contains = assertion
                    .parameters
                    .get("body_contains")
                    .and_then(|v| v.as_str());

                let client = reqwest::Client::new();
                let request = match method.to_uppercase().as_str() {
                    "GET" => client.get(url),
                    "POST" => client.post(url),
                    "PUT" => client.put(url),
                    "DELETE" => client.delete(url),
                    _ => {
                        assertion_result.status = TestStatus::Error;
                        assertion_result.message =
                            Some(format!("Unsupported HTTP method: {}", method));
                        return Ok(assertion_result);
                    }
                };

                match request.send().await {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        let body = response.text().await.unwrap_or_default();

                        let mut passed = true;
                        let mut messages = Vec::new();

                        // Check status code
                        if let Some(expected) = expected_status {
                            if status == expected {
                                messages.push(format!("Status {} matches expected", status));
                            } else {
                                passed = false;
                                messages.push(format!(
                                    "Status {} does not match expected {}",
                                    status, expected
                                ));
                            }
                        } else {
                            messages.push(format!("Response status: {}", status));
                        }

                        // Check body content
                        if let Some(needle) = expected_body_contains {
                            if body.contains(needle) {
                                messages.push(format!("Body contains '{}'", needle));
                            } else {
                                passed = false;
                                messages.push(format!("Body does not contain '{}'", needle));
                            }
                        }

                        assertion_result.status = if passed {
                            TestStatus::Passed
                        } else {
                            TestStatus::Failed
                        };
                        assertion_result.message = Some(messages.join("; "));
                    }
                    Err(e) => {
                        assertion_result.status = TestStatus::Error;
                        assertion_result.message = Some(format!("API request failed: {}", e));
                    }
                }
            }
            AssertionType::JsonPath => {
                self.execute_json_path_assertion(assertion, &mut assertion_result)
                    .await?;
            }
            AssertionType::Custom { assertion_type } => {
                // Extensible custom assertions
                match assertion_type.as_str() {
                    "env_var_set" => {
                        // Check if environment variable is set
                        let key = assertion
                            .parameters
                            .get("key")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                AosError::Config(
                                    "Missing 'key' parameter for env_var_set assertion".to_string(),
                                )
                            })?;

                        if std::env::var(key).is_ok() {
                            assertion_result.status = TestStatus::Passed;
                            assertion_result.message =
                                Some(format!("Environment variable '{}' is set", key));
                        } else {
                            assertion_result.status = TestStatus::Failed;
                            assertion_result.message =
                                Some(format!("Environment variable '{}' is not set", key));
                        }
                    }
                    "env_var_equals" => {
                        // Check if environment variable equals expected value
                        let key = assertion
                            .parameters
                            .get("key")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                AosError::Config(
                                    "Missing 'key' parameter for env_var_equals assertion"
                                        .to_string(),
                                )
                            })?;
                        let expected = assertion
                            .parameters
                            .get("expected")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                AosError::Config(
                                    "Missing 'expected' parameter for env_var_equals assertion"
                                        .to_string(),
                                )
                            })?;

                        match std::env::var(key) {
                            Ok(value) if value == expected => {
                                assertion_result.status = TestStatus::Passed;
                                assertion_result.message =
                                    Some(format!("{}={} matches expected", key, value));
                            }
                            Ok(value) => {
                                assertion_result.status = TestStatus::Failed;
                                assertion_result.message =
                                    Some(format!("{}={}, expected {}", key, value, expected));
                            }
                            Err(_) => {
                                assertion_result.status = TestStatus::Failed;
                                assertion_result.message =
                                    Some(format!("Environment variable '{}' not set", key));
                            }
                        }
                    }
                    "directory_exists" => {
                        let path = assertion
                            .parameters
                            .get("path")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                AosError::Config(
                                    "Missing 'path' parameter for directory_exists assertion"
                                        .to_string(),
                                )
                            })?;

                        let p = std::path::Path::new(path);
                        if p.exists() && p.is_dir() {
                            assertion_result.status = TestStatus::Passed;
                            assertion_result.message = Some(format!("Directory exists: {}", path));
                        } else if p.exists() {
                            assertion_result.status = TestStatus::Failed;
                            assertion_result.message =
                                Some(format!("Path exists but is not a directory: {}", path));
                        } else {
                            assertion_result.status = TestStatus::Failed;
                            assertion_result.message =
                                Some(format!("Directory does not exist: {}", path));
                        }
                    }
                    "file_size" => {
                        let path = assertion
                            .parameters
                            .get("path")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                AosError::Config(
                                    "Missing 'path' parameter for file_size assertion".to_string(),
                                )
                            })?;
                        let min_size = assertion
                            .parameters
                            .get("min_size")
                            .and_then(|v| v.as_u64());
                        let max_size = assertion
                            .parameters
                            .get("max_size")
                            .and_then(|v| v.as_u64());

                        match std::fs::metadata(path) {
                            Ok(meta) => {
                                let size = meta.len();
                                let mut passed = true;
                                let mut messages = Vec::new();

                                if let Some(min) = min_size {
                                    if size >= min {
                                        messages.push(format!("Size {} >= min {}", size, min));
                                    } else {
                                        passed = false;
                                        messages.push(format!("Size {} < min {}", size, min));
                                    }
                                }

                                if let Some(max) = max_size {
                                    if size <= max {
                                        messages.push(format!("Size {} <= max {}", size, max));
                                    } else {
                                        passed = false;
                                        messages.push(format!("Size {} > max {}", size, max));
                                    }
                                }

                                assertion_result.status = if passed {
                                    TestStatus::Passed
                                } else {
                                    TestStatus::Failed
                                };
                                assertion_result.message = Some(messages.join("; "));
                            }
                            Err(e) => {
                                assertion_result.status = TestStatus::Error;
                                assertion_result.message =
                                    Some(format!("Failed to get file metadata: {}", e));
                            }
                        }
                    }
                    _ => {
                        assertion_result.status = TestStatus::Error;
                        assertion_result.message = Some(format!(
                            "Unknown custom assertion type: '{}'. Supported: env_var_set, env_var_equals, directory_exists, file_size",
                            assertion_type
                        ));
                    }
                }
            }
        }

        debug!(
            assertion_id = %assertion.id,
            status = ?assertion_result.status,
            "Test assertion completed"
        );

        Ok(assertion_result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_testing_framework_creation() {
        let config = TestConfig {
            environment_type: TestEnvironmentType::Unit,
            timeout_seconds: 30,
            max_concurrent_tests: 4,
            enable_isolation: true,
            enable_parallelization: true,
            test_data_dir: None,
            fixtures_dir: None,
            additional_config: HashMap::new(),
        };

        let framework = UnifiedTestingFramework::new(config);
        assert!(framework.environments.is_empty());
    }

    #[tokio::test]
    async fn test_test_environment_setup() {
        let config = TestConfig {
            environment_type: TestEnvironmentType::Unit,
            timeout_seconds: 30,
            max_concurrent_tests: 4,
            enable_isolation: true,
            enable_parallelization: true,
            test_data_dir: None,
            fixtures_dir: None,
            additional_config: HashMap::new(),
        };

        let framework = UnifiedTestingFramework::new(config);
        let env = framework.setup(&framework.config).await.unwrap();
        assert_eq!(env.state, EnvironmentState::Initializing);
    }
}
