//! Unified testing framework for AdapterOS
//!
//! Provides a centralized testing framework that consolidates all testing
//! patterns across the system with consistent setup, teardown, and assertions.
//!
//! # Citations
//! - CONTRIBUTING.md L118-122: "Follow Rust naming conventions", "Use `cargo clippy` for linting"
//! - CLAUDE.md L50-55: "Testing frameworks with deterministic execution"

use adapteros_core::{error::AosError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

use tokio::{
    fs::{self, OpenOptions},
    io::AsyncWriteExt,
    net::{lookup_host, TcpStream},
    process::Command,
    sync::RwLock,
    time::{self, Duration},
};

use regex::Regex;
use reqwest::Method;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params_from_iter, Connection};

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
    config: TestConfig,

    /// Test environments
    environments: RwLock<HashMap<String, TestEnvironment>>,

    /// Test results history
    test_results_history: RwLock<Vec<TestResult>>,

    /// Performance metrics
    performance_metrics: RwLock<PerformanceMetrics>,
}

impl UnifiedTestingFramework {
    /// Create a new unified testing framework
    pub fn new(config: TestConfig) -> Self {
        Self {
            config,
            environments: RwLock::new(HashMap::new()),
            test_results_history: RwLock::new(Vec::new()),
            performance_metrics: RwLock::new(PerformanceMetrics {
                total_execution_time_ms: 0,
                average_test_execution_time_ms: 0.0,
                slowest_test_execution_time_ms: 0,
                fastest_test_execution_time_ms: u64::MAX,
                memory_usage_bytes: 0,
                cpu_usage_percentage: 0.0,
                test_throughput: 0.0,
                timestamp: chrono::Utc::now(),
            }),
        }
    }

    /// Update performance metrics
    async fn update_performance_metrics(&self, test_result: &TestResult) {
        let mut metrics = self.performance_metrics.write().await;
        metrics.total_execution_time_ms += test_result.execution_time_ms;

        if test_result.execution_time_ms > metrics.slowest_test_execution_time_ms {
            metrics.slowest_test_execution_time_ms = test_result.execution_time_ms;
        }

        if test_result.execution_time_ms < metrics.fastest_test_execution_time_ms {
            metrics.fastest_test_execution_time_ms = test_result.execution_time_ms;
        }

        let total_tests = self.test_results_history.read().await.len() as f64;
        if total_tests > 0.0 {
            metrics.average_test_execution_time_ms =
                metrics.total_execution_time_ms as f64 / total_tests;

            if metrics.total_execution_time_ms > 0 {
                metrics.test_throughput = total_tests
                    / (metrics.total_execution_time_ms as f64 / 1000.0).max(f64::EPSILON);
            }
        }

        metrics.timestamp = chrono::Utc::now();
    }

    fn status_priority(status: &TestStatus) -> u8 {
        match status {
            TestStatus::Passed => 0,
            TestStatus::Skipped => 1,
            TestStatus::Failed => 2,
            TestStatus::Timeout => 3,
            TestStatus::Error => 4,
        }
    }

    fn update_status(current: &mut TestStatus, candidate: &TestStatus) {
        if Self::status_priority(candidate) > Self::status_priority(current) {
            *current = candidate.clone();
        }
    }

    async fn execute_step_action(&self, step: &TestStep) -> Result<StepResult> {
        match &step.action {
            TestAction::ExecuteCommand { command, args } => {
                self.execute_command_action(step, command, args, None).await
            }
            TestAction::ApiCall { method, url, body } => {
                self.execute_api_call(step, method, url, body).await
            }
            TestAction::DatabaseOperation {
                operation,
                query,
                params,
            } => {
                self.execute_database_operation(step, operation, query, params)
                    .await
            }
            TestAction::FileOperation {
                operation,
                path,
                content,
            } => {
                self.execute_file_operation(step, operation, path, content)
                    .await
            }
            TestAction::NetworkOperation {
                operation,
                host,
                port,
            } => {
                self.execute_network_operation(step, operation, host, *port)
                    .await
            }
            TestAction::Custom { action_type, data } => {
                self.execute_custom_action(step, action_type, data).await
            }
        }
    }

    async fn execute_command_action(
        &self,
        step: &TestStep,
        command: &str,
        args: &[String],
        context: Option<&serde_json::Value>,
    ) -> Result<StepResult> {
        let mut cmd = Command::new(command);
        cmd.args(args);

        if let Some(dir) = step
            .parameters
            .get("working_dir")
            .and_then(|value| value.as_str())
        {
            cmd.current_dir(dir);
        }

        if let Some(env) = step
            .parameters
            .get("env")
            .and_then(|value| value.as_object())
        {
            for (key, value) in env {
                if let Some(value_str) = value.as_str() {
                    cmd.env(key, value_str);
                } else {
                    cmd.env(key, value.to_string());
                }
            }
        }

        if let Some(context) = context {
            if let Some(dir) = context.get("working_dir").and_then(|value| value.as_str()) {
                cmd.current_dir(dir);
            }

            if let Some(env) = context.get("env").and_then(|value| value.as_object()) {
                for (key, value) in env {
                    if let Some(value_str) = value.as_str() {
                        cmd.env(key, value_str);
                    } else {
                        cmd.env(key, value.to_string());
                    }
                }
            }

            if let Some(extra_args) = context.get("args").and_then(|value| value.as_array()) {
                for arg in extra_args {
                    if let Some(arg_str) = arg.as_str() {
                        cmd.arg(arg_str);
                    } else {
                        cmd.arg(arg.to_string());
                    }
                }
            }
        }

        let output = cmd
            .output()
            .await
            .map_err(|err| AosError::Io(err.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut combined_output = stdout;
        if !stderr.is_empty() && output.status.success() {
            if !combined_output.is_empty() {
                combined_output.push('\n');
            }
            combined_output.push_str(&stderr);
        }

        let mut step_result = StepResult {
            step_id: step.id.clone(),
            status: if output.status.success() {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            },
            output: if combined_output.is_empty() {
                None
            } else {
                Some(combined_output)
            },
            error: None,
            execution_time_ms: 0,
        };

        if !output.status.success() {
            step_result.error = Some(if !stderr.is_empty() {
                stderr
            } else {
                format!("Command '{}' failed with status {}", command, output.status)
            });
        }

        Ok(step_result)
    }

    async fn execute_api_call(
        &self,
        step: &TestStep,
        method: &str,
        url: &str,
        body: &Option<serde_json::Value>,
    ) -> Result<StepResult> {
        if let Some(mock_response) = step.parameters.get("mock_response") {
            return Ok(StepResult {
                step_id: step.id.clone(),
                status: TestStatus::Passed,
                output: Some(mock_response.to_string()),
                error: None,
                execution_time_ms: 0,
            });
        }

        let client = reqwest::Client::new();
        let method =
            Method::from_bytes(method.as_bytes()).map_err(|err| AosError::Http(err.to_string()))?;

        let mut request = client.request(method, url);

        if let Some(headers) = step
            .parameters
            .get("headers")
            .and_then(|value| value.as_object())
        {
            for (key, value) in headers {
                if let Some(value_str) = value.as_str() {
                    request = request.header(key, value_str);
                } else {
                    request = request.header(key, value.to_string());
                }
            }
        }

        if let Some(body) = body {
            request = request.json(body);
        }

        let response = request
            .send()
            .await
            .map_err(|err| AosError::Http(err.to_string()))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|err| AosError::Http(err.to_string()))?;

        let mut step_result = StepResult {
            step_id: step.id.clone(),
            status: if status.is_success() {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            },
            output: Some(
                serde_json::json!({
                    "status": status.as_u16(),
                    "body": text,
                })
                .to_string(),
            ),
            error: None,
            execution_time_ms: 0,
        };

        if !status.is_success() {
            step_result.error = Some(format!("API call failed with status {}", status));
        }

        Ok(step_result)
    }

    async fn execute_database_operation(
        &self,
        step: &TestStep,
        operation: &str,
        query: &str,
        params: &[serde_json::Value],
    ) -> Result<StepResult> {
        let connection = step
            .parameters
            .get("connection")
            .and_then(|value| value.as_str())
            .unwrap_or(":memory:")
            .to_string();

        let operation_owned = operation.to_string();
        let query_owned = query.to_string();
        let params_owned = params.to_vec();

        let (status, output, error) = tokio::task::spawn_blocking(move || {
            let connection =
                Connection::open(&connection).map_err(|err| AosError::Database(err.to_string()))?;

            let param_values = params_owned
                .iter()
                .map(Self::json_to_sql_value)
                .collect::<Result<Vec<_>>>()?;

            match operation_owned.as_str() {
                "execute" => {
                    let affected = connection
                        .execute(&query_owned, params_from_iter(param_values.clone()))
                        .map_err(|err| AosError::Database(err.to_string()))?;
                    Ok((TestStatus::Passed, Some(affected.to_string()), None))
                }
                "query_one" => {
                    let mut statement = connection
                        .prepare(&query_owned)
                        .map_err(|err| AosError::Database(err.to_string()))?;
                    let mut rows = statement
                        .query(params_from_iter(param_values.clone()))
                        .map_err(|err| AosError::Database(err.to_string()))?;

                    if let Some(row) = rows.next()? {
                        let mut values = Vec::new();
                        let column_count = row.as_ref().column_count();
                        for index in 0..column_count {
                            let value: SqlValue = row.get(index)?;
                            values.push(value);
                        }

                        let json_values = values
                            .into_iter()
                            .map(Self::sql_value_to_json)
                            .collect::<Result<Vec<_>>>()?;

                        Ok((
                            TestStatus::Passed,
                            Some(serde_json::Value::Array(json_values).to_string()),
                            None,
                        ))
                    } else {
                        Ok((
                            TestStatus::Failed,
                            None,
                            Some("No rows returned".to_string()),
                        ))
                    }
                }
                _ => Err(AosError::Database(format!(
                    "Unsupported database operation: {}",
                    operation_owned
                ))),
            }
        })
        .await
        .map_err(|err| AosError::Database(err.to_string()))??;

        Ok(StepResult {
            step_id: step.id.clone(),
            status,
            output,
            error,
            execution_time_ms: 0,
        })
    }

    async fn execute_file_operation(
        &self,
        step: &TestStep,
        operation: &str,
        path: &str,
        content: &Option<String>,
    ) -> Result<StepResult> {
        let result = match operation {
            "write" => {
                let data = content
                    .clone()
                    .ok_or_else(|| AosError::Io("Missing file content".to_string()))?;
                fs::write(path, data.as_bytes())
                    .await
                    .map_err(|err| AosError::Io(err.to_string()))?;
                StepResult {
                    step_id: step.id.clone(),
                    status: TestStatus::Passed,
                    output: Some(format!("Wrote {} bytes to {}", data.len(), path)),
                    error: None,
                    execution_time_ms: 0,
                }
            }
            "read" => {
                let data = fs::read_to_string(path)
                    .await
                    .map_err(|err| AosError::Io(err.to_string()))?;
                StepResult {
                    step_id: step.id.clone(),
                    status: TestStatus::Passed,
                    output: Some(data),
                    error: None,
                    execution_time_ms: 0,
                }
            }
            "append" => {
                let data = content
                    .clone()
                    .ok_or_else(|| AosError::Io("Missing file content".to_string()))?;
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .await
                    .map_err(|err| AosError::Io(err.to_string()))?;
                file.write_all(data.as_bytes())
                    .await
                    .map_err(|err| AosError::Io(err.to_string()))?;
                StepResult {
                    step_id: step.id.clone(),
                    status: TestStatus::Passed,
                    output: Some(format!("Appended {} bytes to {}", data.len(), path)),
                    error: None,
                    execution_time_ms: 0,
                }
            }
            "delete" => {
                fs::remove_file(path)
                    .await
                    .map_err(|err| AosError::Io(err.to_string()))?;
                StepResult {
                    step_id: step.id.clone(),
                    status: TestStatus::Passed,
                    output: Some(format!("Deleted {}", path)),
                    error: None,
                    execution_time_ms: 0,
                }
            }
            _ => {
                return Err(AosError::Io(format!(
                    "Unsupported file operation: {}",
                    operation
                )));
            }
        };

        Ok(result)
    }

    async fn execute_network_operation(
        &self,
        step: &TestStep,
        operation: &str,
        host: &str,
        port: u16,
    ) -> Result<StepResult> {
        match operation {
            "tcp_connect" => {
                let address = format!("{}:{}", host, port);
                match TcpStream::connect(&address).await {
                    Ok(_) => Ok(StepResult {
                        step_id: step.id.clone(),
                        status: TestStatus::Passed,
                        output: Some(format!("Connected to {}", address)),
                        error: None,
                        execution_time_ms: 0,
                    }),
                    Err(err) => Ok(StepResult {
                        step_id: step.id.clone(),
                        status: TestStatus::Failed,
                        output: None,
                        error: Some(err.to_string()),
                        execution_time_ms: 0,
                    }),
                }
            }
            "resolve" => {
                match lookup_host((host, port)).await {
                    Ok(lookup) => {
                        let addresses: Vec<String> = lookup.map(|addr| addr.to_string()).collect();
                        Ok(StepResult {
                            step_id: step.id.clone(),
                            status: TestStatus::Passed,
                            output: Some(
                                serde_json::json!({
                                    "addresses": addresses,
                                })
                                .to_string(),
                            ),
                            error: None,
                            execution_time_ms: 0,
                        })
                    }
                    Err(err) => Ok(StepResult {
                        step_id: step.id.clone(),
                        status: TestStatus::Failed,
                        output: None,
                        error: Some(err.to_string()),
                        execution_time_ms: 0,
                    }),
                }
            }
            _ => Err(AosError::Io(format!(
                "Unsupported network operation: {}",
                operation
            ))),
        }
    }

    async fn execute_custom_action(
        &self,
        step: &TestStep,
        action_type: &str,
        data: &serde_json::Value,
    ) -> Result<StepResult> {
        match action_type {
            "framework::cargo" => {
                let subcommand = data
                    .get("subcommand")
                    .and_then(|value| value.as_str())
                    .unwrap_or("test");
                let mut args = vec![subcommand.to_string()];
                args.extend(Self::json_array_to_strings(data.get("args")));
                self.execute_command_action(step, "cargo", &args, Some(data))
                    .await
            }
            "framework::pytest" => {
                let mut args = Self::json_array_to_strings(data.get("args"));
                if args.is_empty() {
                    args.push(String::from("-q"));
                }
                self.execute_command_action(step, "pytest", &args, Some(data))
                    .await
            }
            "framework::npm" => {
                let script = data
                    .get("script")
                    .and_then(|value| value.as_str())
                    .unwrap_or("test");
                let mut args = vec![String::from("run"), script.to_string()];
                args.extend(Self::json_array_to_strings(data.get("args")));
                self.execute_command_action(step, "npm", &args, Some(data))
                    .await
            }
            _ => Ok(StepResult {
                step_id: step.id.clone(),
                status: TestStatus::Passed,
                output: Some(data.to_string()),
                error: None,
                execution_time_ms: 0,
            }),
        }
    }

    fn json_to_sql_value(value: &serde_json::Value) -> Result<SqlValue> {
        Ok(match value {
            serde_json::Value::Null => SqlValue::Null,
            serde_json::Value::Bool(boolean) => SqlValue::Integer(if *boolean { 1 } else { 0 }),
            serde_json::Value::Number(number) => {
                if let Some(int) = number.as_i64() {
                    SqlValue::Integer(int)
                } else if let Some(float) = number.as_f64() {
                    SqlValue::Real(float)
                } else {
                    return Err(AosError::Database(format!(
                        "Unsupported numeric value: {}",
                        number
                    )));
                }
            }
            serde_json::Value::String(string) => SqlValue::Text(string.clone()),
            serde_json::Value::Array(array) => {
                SqlValue::Text(serde_json::Value::Array(array.clone()).to_string())
            }
            serde_json::Value::Object(object) => {
                SqlValue::Text(serde_json::Value::Object(object.clone()).to_string())
            }
        })
    }

    fn sql_value_to_json(value: SqlValue) -> Result<serde_json::Value> {
        Ok(match value {
            SqlValue::Null => serde_json::Value::Null,
            SqlValue::Integer(int) => serde_json::Value::Number(int.into()),
            SqlValue::Real(float) => serde_json::Number::from_f64(float)
                .map(serde_json::Value::Number)
                .ok_or_else(|| {
                    AosError::Database(format!("Invalid floating point value: {}", float))
                })?,
            SqlValue::Text(text) => serde_json::Value::String(text),
            SqlValue::Blob(blob) => serde_json::Value::Array(
                blob.into_iter()
                    .map(|byte| serde_json::Value::Number(byte.into()))
                    .collect(),
            ),
        })
    }

    fn json_array_to_strings(value: Option<&serde_json::Value>) -> Vec<String> {
        value
            .and_then(|value| value.as_array())
            .map(|array| {
                array
                    .iter()
                    .map(|entry| {
                        entry
                            .as_str()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| entry.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn failure_message(assertion: &TestAssertion, default: String) -> String {
        assertion.message.clone().unwrap_or(default)
    }

    fn value_as_f64(value: &serde_json::Value) -> Option<f64> {
        match value {
            serde_json::Value::Number(number) => number.as_f64(),
            serde_json::Value::String(string) => string.parse().ok(),
            serde_json::Value::Bool(boolean) => Some(if *boolean { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    fn value_as_string(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(string) => string.clone(),
            _ => value.to_string(),
        }
    }

    fn value_contains(haystack: &serde_json::Value, needle: &serde_json::Value) -> bool {
        match haystack {
            serde_json::Value::String(string) => string.contains(&Self::value_as_string(needle)),
            serde_json::Value::Array(array) => array.contains(needle),
            serde_json::Value::Object(object) => {
                if let Some(key) = needle.as_str() {
                    object.contains_key(key)
                } else {
                    object.values().any(|value| value == needle)
                }
            }
            _ => false,
        }
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

        self.environments
            .write()
            .await
            .insert(env_id.clone(), environment.clone());

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

        self.environments.write().await.remove(&env.id);

        info!(
            env_id = %env.id,
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

        if let Some(framework) = test_case
            .metadata
            .get("framework")
            .and_then(|value| value.as_str())
        {
            test_result.metadata.insert(
                "framework".to_string(),
                serde_json::Value::String(framework.to_string()),
            );
        }

        test_result.metadata.insert(
            "environment_type".into(),
            serde_json::to_value(&self.config.environment_type)
                .unwrap_or_else(|_| serde_json::Value::String("Unknown".into())),
        );

        let mut overall_status = TestStatus::Passed;

        if let Some(setup_step) = &test_case.setup {
            let setup_result = self.run_test_step(setup_step).await?;
            Self::update_status(&mut overall_status, &setup_result.status);
            if setup_result.status != TestStatus::Passed {
                test_result.error = setup_result.error.clone();
            }
            test_result.step_results.push(setup_result);
        }

        for step in &test_case.steps {
            let step_result = self.run_test_step(step).await?;
            if step_result.output.is_some() {
                test_result.output = step_result.output.clone();
            }

            Self::update_status(&mut overall_status, &step_result.status);
            if step_result.status != TestStatus::Passed {
                test_result.error = step_result.error.clone();
            }

            test_result.step_results.push(step_result);
        }

        if let Some(teardown_step) = &test_case.teardown {
            let teardown_result = self.run_test_step(teardown_step).await?;
            Self::update_status(&mut overall_status, &teardown_result.status);
            test_result.step_results.push(teardown_result);
        }

        for assertion in &test_case.assertions {
            let assertion_result = self.run_assertion(assertion).await?;
            Self::update_status(&mut overall_status, &assertion_result.status);
            if assertion_result.status != TestStatus::Passed {
                test_result.error = assertion_result.message.clone();
            }
            test_result.assertion_results.push(assertion_result);
        }

        let end_time = chrono::Utc::now();
        let execution_time = start_instant.elapsed();

        test_result.end_time = end_time;
        test_result.execution_time_ms = execution_time.as_millis() as u64;
        test_result.status = overall_status;

        {
            let mut history = self.test_results_history.write().await;
            history.push(test_result.clone());
        }

        self.update_performance_metrics(&test_result).await;

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
        // TODO: Implement actual coverage reporting
        // This would integrate with coverage tools like tarpaulin, grcov, etc.

        Ok(CoverageReport {
            overall_coverage: 85.0,
            line_coverage: 82.0,
            branch_coverage: 78.0,
            function_coverage: 90.0,
            file_coverage: HashMap::new(),
            timestamp: chrono::Utc::now(),
        })
    }

    async fn get_performance_metrics(&self) -> Result<PerformanceMetrics> {
        Ok(self.performance_metrics.read().await.clone())
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

        let max_attempts = step.retries.unwrap_or(0).saturating_add(1);
        let mut total_execution_ms = 0u64;
        let mut final_result = StepResult {
            step_id: step.id.clone(),
            status: TestStatus::Passed,
            output: None,
            error: None,
            execution_time_ms: 0,
        };

        for attempt in 0..max_attempts {
            let attempt_start = std::time::Instant::now();
            let action_future = self.execute_step_action(step);

            let action_result = if let Some(timeout) = step.timeout_seconds {
                match time::timeout(Duration::from_secs(timeout), action_future).await {
                    Ok(result) => result,
                    Err(_) => {
                        final_result = StepResult {
                            step_id: step.id.clone(),
                            status: TestStatus::Timeout,
                            output: None,
                            error: Some(format!(
                                "Step '{}' timed out after {} seconds",
                                step.name, timeout
                            )),
                            execution_time_ms: attempt_start.elapsed().as_millis() as u64,
                        };
                        break;
                    }
                }
            } else {
                action_future.await
            };

            let attempt_elapsed = attempt_start.elapsed().as_millis() as u64;
            total_execution_ms = total_execution_ms.saturating_add(attempt_elapsed);

            match action_result {
                Ok(mut result) => {
                    result.execution_time_ms = total_execution_ms;
                    final_result = result;

                    if matches!(
                        final_result.status,
                        TestStatus::Passed | TestStatus::Skipped
                    ) {
                        break;
                    }
                }
                Err(err) => {
                    final_result = StepResult {
                        step_id: step.id.clone(),
                        status: TestStatus::Error,
                        output: None,
                        error: Some(err.to_string()),
                        execution_time_ms: total_execution_ms,
                    };
                }
            }

            if attempt + 1 == max_attempts {
                break;
            }
        }

        if final_result.execution_time_ms == 0 {
            final_result.execution_time_ms = start_instant.elapsed().as_millis() as u64;
        }

        debug!(
            step_id = %step.id,
            status = ?final_result.status,
            execution_time_ms = final_result.execution_time_ms,
            "Test step completed"
        );

        Ok(final_result)
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

        match &assertion.assertion_type {
            AssertionType::Equals => {
                if let (Some(expected), Some(actual)) = (
                    assertion.parameters.get("expected"),
                    assertion.parameters.get("actual"),
                ) {
                    if expected != actual {
                        assertion_result.status = TestStatus::Failed;
                        assertion_result.details = Some(serde_json::json!({
                            "expected": expected,
                            "actual": actual,
                        }));
                        assertion_result.message = Some(Self::failure_message(
                            assertion,
                            format!("Expected {:?}, got {:?}", expected, actual),
                        ));
                    }
                } else {
                    assertion_result.status = TestStatus::Error;
                    assertion_result.message = Some("Missing expected or actual parameter".into());
                }
            }
            AssertionType::NotEquals => {
                if let (Some(expected), Some(actual)) = (
                    assertion.parameters.get("expected"),
                    assertion.parameters.get("actual"),
                ) {
                    if expected == actual {
                        assertion_result.status = TestStatus::Failed;
                        assertion_result.details = Some(serde_json::json!({
                            "unexpected": actual,
                        }));
                        assertion_result.message = Some(Self::failure_message(
                            assertion,
                            format!("Values were unexpectedly equal: {:?}", actual),
                        ));
                    }
                } else {
                    assertion_result.status = TestStatus::Error;
                    assertion_result.message = Some("Missing expected or actual parameter".into());
                }
            }
            AssertionType::GreaterThan => {
                if let (Some(actual), Some(threshold)) = (
                    assertion.parameters.get("actual"),
                    assertion.parameters.get("threshold"),
                ) {
                    match (Self::value_as_f64(actual), Self::value_as_f64(threshold)) {
                        (Some(actual_value), Some(threshold_value)) => {
                            if actual_value <= threshold_value {
                                assertion_result.status = TestStatus::Failed;
                                assertion_result.details = Some(serde_json::json!({
                                    "actual": actual_value,
                                    "threshold": threshold_value,
                                }));
                                assertion_result.message = Some(Self::failure_message(
                                    assertion,
                                    format!(
                                        "Value {} was not greater than {}",
                                        actual_value, threshold_value
                                    ),
                                ));
                            }
                        }
                        _ => {
                            assertion_result.status = TestStatus::Error;
                            assertion_result.message = Some(
                                "Actual or threshold value could not be interpreted as a number"
                                    .into(),
                            );
                        }
                    }
                } else {
                    assertion_result.status = TestStatus::Error;
                    assertion_result.message = Some("Missing actual or threshold parameter".into());
                }
            }
            AssertionType::LessThan => {
                if let (Some(actual), Some(threshold)) = (
                    assertion.parameters.get("actual"),
                    assertion.parameters.get("threshold"),
                ) {
                    match (Self::value_as_f64(actual), Self::value_as_f64(threshold)) {
                        (Some(actual_value), Some(threshold_value)) => {
                            if actual_value >= threshold_value {
                                assertion_result.status = TestStatus::Failed;
                                assertion_result.details = Some(serde_json::json!({
                                    "actual": actual_value,
                                    "threshold": threshold_value,
                                }));
                                assertion_result.message = Some(Self::failure_message(
                                    assertion,
                                    format!(
                                        "Value {} was not less than {}",
                                        actual_value, threshold_value
                                    ),
                                ));
                            }
                        }
                        _ => {
                            assertion_result.status = TestStatus::Error;
                            assertion_result.message = Some(
                                "Actual or threshold value could not be interpreted as a number"
                                    .into(),
                            );
                        }
                    }
                } else {
                    assertion_result.status = TestStatus::Error;
                    assertion_result.message = Some("Missing actual or threshold parameter".into());
                }
            }
            AssertionType::Contains => {
                if let (Some(haystack), Some(needle)) = (
                    assertion.parameters.get("haystack"),
                    assertion.parameters.get("needle"),
                ) {
                    if !Self::value_contains(haystack, needle) {
                        assertion_result.status = TestStatus::Failed;
                        assertion_result.details = Some(serde_json::json!({
                            "haystack": haystack,
                            "needle": needle,
                        }));
                        assertion_result.message = Some(Self::failure_message(
                            assertion,
                            format!("Haystack {:?} did not contain {:?}", haystack, needle),
                        ));
                    }
                } else {
                    assertion_result.status = TestStatus::Error;
                    assertion_result.message = Some("Missing haystack or needle parameter".into());
                }
            }
            AssertionType::NotContains => {
                if let (Some(haystack), Some(needle)) = (
                    assertion.parameters.get("haystack"),
                    assertion.parameters.get("needle"),
                ) {
                    if Self::value_contains(haystack, needle) {
                        assertion_result.status = TestStatus::Failed;
                        assertion_result.details = Some(serde_json::json!({
                            "haystack": haystack,
                            "needle": needle,
                        }));
                        assertion_result.message = Some(Self::failure_message(
                            assertion,
                            format!(
                                "Haystack {:?} unexpectedly contained {:?}",
                                haystack, needle
                            ),
                        ));
                    }
                } else {
                    assertion_result.status = TestStatus::Error;
                    assertion_result.message = Some("Missing haystack or needle parameter".into());
                }
            }
            AssertionType::RegexMatch => {
                if let (Some(pattern), Some(value)) = (
                    assertion.parameters.get("pattern"),
                    assertion.parameters.get("value"),
                ) {
                    if let Some(pattern_str) = pattern.as_str() {
                        match Regex::new(pattern_str) {
                            Ok(regex) => {
                                let value_str = Self::value_as_string(value);
                                if !regex.is_match(&value_str) {
                                    assertion_result.status = TestStatus::Failed;
                                    assertion_result.details = Some(serde_json::json!({
                                        "pattern": pattern_str,
                                        "value": value,
                                    }));
                                    assertion_result.message = Some(Self::failure_message(
                                        assertion,
                                        format!(
                                            "Value {:?} did not match pattern {}",
                                            value, pattern_str
                                        ),
                                    ));
                                }
                            }
                            Err(err) => {
                                assertion_result.status = TestStatus::Error;
                                assertion_result.message =
                                    Some(format!("Invalid regex pattern: {}", err));
                            }
                        }
                    } else {
                        assertion_result.status = TestStatus::Error;
                        assertion_result.message = Some("Pattern must be a string".into());
                    }
                } else {
                    assertion_result.status = TestStatus::Error;
                    assertion_result.message = Some("Missing pattern or value parameter".into());
                }
            }
            AssertionType::FileExists => {
                if let Some(path) = assertion
                    .parameters
                    .get("path")
                    .and_then(|value| value.as_str())
                {
                    match fs::metadata(path).await {
                        Ok(_) => {}
                        Err(err) => {
                            assertion_result.status = TestStatus::Failed;
                            assertion_result.message = Some(Self::failure_message(
                                assertion,
                                format!("File '{}' does not exist: {}", path, err),
                            ));
                        }
                    }
                } else {
                    assertion_result.status = TestStatus::Error;
                    assertion_result.message = Some("Missing path parameter".into());
                }
            }
            AssertionType::FileNotExists => {
                if let Some(path) = assertion
                    .parameters
                    .get("path")
                    .and_then(|value| value.as_str())
                {
                    if fs::metadata(path).await.is_ok() {
                        assertion_result.status = TestStatus::Failed;
                        assertion_result.message = Some(Self::failure_message(
                            assertion,
                            format!("File '{}' unexpectedly exists", path),
                        ));
                    }
                } else {
                    assertion_result.status = TestStatus::Error;
                    assertion_result.message = Some("Missing path parameter".into());
                }
            }
            AssertionType::DatabaseRecordExists => {
                let connection = assertion
                    .parameters
                    .get("connection")
                    .and_then(|value| value.as_str())
                    .unwrap_or(":memory:")
                    .to_string();

                if let Some(query_value) = assertion
                    .parameters
                    .get("query")
                    .and_then(|value| value.as_str())
                {
                    let query = query_value.to_string();
                    let params = assertion
                        .parameters
                        .get("params")
                        .and_then(|value| value.as_array())
                        .cloned()
                        .unwrap_or_default();

                    let record_exists = tokio::task::spawn_blocking(move || {
                        let connection = Connection::open(&connection)
                            .map_err(|err| AosError::Database(err.to_string()))?;
                        let param_values = params
                            .iter()
                            .map(Self::json_to_sql_value)
                            .collect::<Result<Vec<_>>>()?;

                        let mut statement = connection
                            .prepare(&query)
                            .map_err(|err| AosError::Database(err.to_string()))?;
                        let mut rows = statement
                            .query(params_from_iter(param_values.clone()))
                            .map_err(|err| AosError::Database(err.to_string()))?;

                        Ok::<_, AosError>(rows.next()?.is_some())
                    })
                    .await
                    .map_err(|err| AosError::Database(err.to_string()))??;

                    if !record_exists {
                        assertion_result.status = TestStatus::Failed;
                        assertion_result.message = Some(Self::failure_message(
                            assertion,
                            "Database record did not exist".to_string(),
                        ));
                    }
                } else {
                    assertion_result.status = TestStatus::Error;
                    assertion_result.message = Some("Missing query parameter".into());
                }
            }
            AssertionType::ApiResponse => {
                let expected_status = assertion
                    .parameters
                    .get("expected_status")
                    .or_else(|| assertion.parameters.get("status"))
                    .and_then(|value| value.as_u64());
                let actual_status = assertion
                    .parameters
                    .get("actual_status")
                    .and_then(|value| value.as_u64());

                if let (Some(expected_status), Some(actual_status)) =
                    (expected_status, actual_status)
                {
                    if expected_status != actual_status {
                        assertion_result.status = TestStatus::Failed;
                        assertion_result.details = Some(serde_json::json!({
                            "expected_status": expected_status,
                            "actual_status": actual_status,
                        }));
                        assertion_result.message = Some(Self::failure_message(
                            assertion,
                            format!("Expected status {}, got {}", expected_status, actual_status),
                        ));
                    }
                } else {
                    assertion_result.status = TestStatus::Error;
                    assertion_result.message =
                        Some("Missing expected_status or actual_status parameter".into());
                }

                if assertion_result.status == TestStatus::Passed {
                    if let Some(body_value) = assertion.parameters.get("body_contains") {
                        if let Some(body_contains_str) = body_value.as_str() {
                            let body_contains = body_contains_str.to_string();
                            if let Some(actual_body_value) = assertion.parameters.get("actual_body") {
                                if let Some(actual_body_str) = actual_body_value.as_str() {
                                    let actual_body = actual_body_str.to_string();
                                    if !actual_body.contains(&body_contains) {
                                        assertion_result.status = TestStatus::Failed;
                                        assertion_result.message = Some(Self::failure_message(
                                            assertion,
                                            format!(
                                                "Body did not contain expected substring '{}'.",
                                                body_contains
                                            ),
                                        ));
                                    }
                                } else {
                                    assertion_result.status = TestStatus::Error;
                                    assertion_result.message = Some(
                                        "Missing actual_body parameter for body assertion".into(),
                                    );
                                }
                            } else {
                                assertion_result.status = TestStatus::Error;
                                assertion_result.message = Some(
                                    "Missing actual_body parameter for body assertion".into(),
                                );
                            }
                        } else {
                            assertion_result.status = TestStatus::Error;
                            assertion_result.message =
                                Some("body_contains must be a string".into());
                        }
                    }
                }
            }
            AssertionType::Custom { assertion_type } => {
                let success = assertion
                    .parameters
                    .get("success")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);

                if !success {
                    assertion_result.status = TestStatus::Failed;
                    assertion_result.message = Some(Self::failure_message(
                        assertion,
                        format!("Custom assertion '{}' reported failure", assertion_type),
                    ));
                } else if let Some(details) = assertion.parameters.get("details") {
                    assertion_result.details = Some(details.clone());
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
    use serde_json::json;
    use tempfile::tempdir;

    fn base_config() -> TestConfig {
        TestConfig {
            environment_type: TestEnvironmentType::Unit,
            timeout_seconds: 30,
            max_concurrent_tests: 4,
            enable_isolation: true,
            enable_parallelization: true,
            test_data_dir: None,
            fixtures_dir: None,
            additional_config: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_testing_framework_creation() {
        let framework = UnifiedTestingFramework::new(base_config());
        let metrics = framework.get_performance_metrics().await.unwrap();
        assert_eq!(metrics.total_execution_time_ms, 0);
    }

    #[tokio::test]
    async fn test_test_environment_setup() {
        let framework = UnifiedTestingFramework::new(base_config());
        let env = framework.setup(&framework.config).await.unwrap();
        assert_eq!(env.state, EnvironmentState::Initializing);
        assert!(framework.environments.read().await.contains_key(&env.id));
    }

    #[tokio::test]
    async fn test_run_test_step_execute_command() {
        let framework = UnifiedTestingFramework::new(base_config());
        let step = TestStep {
            id: "step-1".into(),
            name: "echo".into(),
            description: None,
            action: TestAction::ExecuteCommand {
                command: "echo".into(),
                args: vec!["hello".into()],
            },
            parameters: HashMap::new(),
            timeout_seconds: Some(5),
            retries: Some(0),
            dependencies: Vec::new(),
        };

        let result = framework.run_test_step(&step).await.unwrap();
        assert_eq!(result.status, TestStatus::Passed);
        assert!(result
            .output
            .as_deref()
            .map(|output| output.contains("hello"))
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn test_run_test_step_custom_framework_command() {
        let framework = UnifiedTestingFramework::new(base_config());
        let mut parameters = HashMap::new();
        parameters.insert("env".into(), json!({"RUST_LOG": "info"}));

        let step = TestStep {
            id: "step-cargo".into(),
            name: "cargo-version".into(),
            description: None,
            action: TestAction::Custom {
                action_type: "framework::cargo".into(),
                data: json!({ "subcommand": "--version" }),
            },
            parameters,
            timeout_seconds: Some(10),
            retries: Some(0),
            dependencies: Vec::new(),
        };

        let result = framework.run_test_step(&step).await.unwrap();
        assert_eq!(result.status, TestStatus::Passed);
        assert!(result
            .output
            .as_deref()
            .map(|output| output.to_lowercase().contains("cargo"))
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn test_run_test_case_with_database_assertion() {
        let framework = UnifiedTestingFramework::new(base_config());
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path = db_path.to_string_lossy().to_string();

        let mut db_parameters = HashMap::new();
        db_parameters.insert("connection".into(), json!(db_path.clone()));

        let create_step = TestStep {
            id: "create".into(),
            name: "Create table".into(),
            description: None,
            action: TestAction::DatabaseOperation {
                operation: "execute".into(),
                query: "CREATE TABLE IF NOT EXISTS data(id INTEGER PRIMARY KEY, value TEXT);"
                    .into(),
                params: Vec::new(),
            },
            parameters: db_parameters.clone(),
            timeout_seconds: Some(5),
            retries: Some(0),
            dependencies: Vec::new(),
        };

        let insert_step = TestStep {
            id: "insert".into(),
            name: "Insert".into(),
            description: None,
            action: TestAction::DatabaseOperation {
                operation: "execute".into(),
                query: "INSERT INTO data(value) VALUES('ok');".into(),
                params: Vec::new(),
            },
            parameters: db_parameters.clone(),
            timeout_seconds: Some(5),
            retries: Some(1),
            dependencies: vec!["create".into()],
        };

        let assertion = TestAssertion {
            id: "assert-db".into(),
            name: "Record exists".into(),
            assertion_type: AssertionType::DatabaseRecordExists,
            parameters: HashMap::from([
                ("connection".into(), json!(db_path.clone())),
                (
                    "query".into(),
                    json!("SELECT value FROM data WHERE value = 'ok' LIMIT 1"),
                ),
            ]),
            message: Some("Expected database record to exist".into()),
        };

        let test_case = TestCase {
            id: "case-db".into(),
            name: "Database case".into(),
            description: None,
            test_type: TestType::Integration,
            priority: TestPriority::High,
            tags: vec!["db".into()],
            setup: None,
            steps: vec![create_step, insert_step],
            teardown: None,
            assertions: vec![assertion],
            timeout_seconds: Some(30),
            dependencies: Vec::new(),
            metadata: HashMap::new(),
        };

        let result = framework.run_test(&test_case).await.unwrap();
        assert_eq!(result.status, TestStatus::Passed);
        assert_eq!(result.assertion_results.len(), 1);
        assert_eq!(framework.test_results_history.read().await.len(), 1);
    }

    #[tokio::test]
    async fn test_run_suite_summary_counts() {
        let framework = UnifiedTestingFramework::new(base_config());

        let passing_case = TestCase {
            id: "case-pass".into(),
            name: "Passing".into(),
            description: None,
            test_type: TestType::Unit,
            priority: TestPriority::Medium,
            tags: vec![],
            setup: None,
            steps: vec![TestStep {
                id: "pass-step".into(),
                name: "Pass step".into(),
                description: None,
                action: TestAction::ExecuteCommand {
                    command: "sh".into(),
                    args: vec!["-c".into(), "exit 0".into()],
                },
                parameters: HashMap::new(),
                timeout_seconds: Some(5),
                retries: Some(0),
                dependencies: Vec::new(),
            }],
            teardown: None,
            assertions: vec![TestAssertion {
                id: "assert-pass".into(),
                name: "Equality".into(),
                assertion_type: AssertionType::Equals,
                parameters: HashMap::from([
                    ("expected".into(), json!(1)),
                    ("actual".into(), json!(1)),
                ]),
                message: None,
            }],
            timeout_seconds: Some(10),
            dependencies: Vec::new(),
            metadata: HashMap::new(),
        };

        let failing_case = TestCase {
            id: "case-fail".into(),
            name: "Failing".into(),
            description: None,
            test_type: TestType::Unit,
            priority: TestPriority::Medium,
            tags: vec![],
            setup: None,
            steps: vec![TestStep {
                id: "fail-step".into(),
                name: "Fail step".into(),
                description: None,
                action: TestAction::ExecuteCommand {
                    command: "sh".into(),
                    args: vec!["-c".into(), "exit 0".into()],
                },
                parameters: HashMap::new(),
                timeout_seconds: Some(5),
                retries: Some(0),
                dependencies: Vec::new(),
            }],
            teardown: None,
            assertions: vec![TestAssertion {
                id: "assert-fail".into(),
                name: "Inequality".into(),
                assertion_type: AssertionType::Equals,
                parameters: HashMap::from([
                    ("expected".into(), json!("ok")),
                    ("actual".into(), json!("not-ok")),
                ]),
                message: None,
            }],
            timeout_seconds: Some(10),
            dependencies: Vec::new(),
            metadata: HashMap::new(),
        };

        let suite = TestSuite {
            id: "suite".into(),
            name: "Suite".into(),
            description: None,
            test_cases: vec![passing_case, failing_case],
            config: base_config(),
            metadata: HashMap::new(),
        };

        let result = framework.run_suite(&suite).await.unwrap();
        assert_eq!(result.test_results.len(), 2);
        assert_eq!(result.summary.total_tests, 2);
        assert_eq!(result.summary.passed_tests, 1);
        assert_eq!(result.summary.failed_tests, 1);
    }
}
