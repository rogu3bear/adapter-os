use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    pub environment_type: TestEnvironmentType,
    pub timeout_seconds: u64,
    pub max_concurrent_tests: u32,
    pub enable_isolation: bool,
    pub enable_parallelization: bool,
    pub test_data_dir: Option<String>,
    pub fixtures_dir: Option<String>,
    pub additional_config: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TestEnvironmentType {
    Unit,
    Integration,
    EndToEnd,
    Performance,
    Security,
    Determinism,
}

#[derive(Debug, Clone)]
pub struct TestEnvironment {
    pub id: String,
    pub environment_type: TestEnvironmentType,
    pub state: EnvironmentState,
    pub resources: HashMap<String, Value>,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EnvironmentState {
    Initializing,
    Ready,
    Running,
    CleaningUp,
    Failed,
    Destroyed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub test_type: TestType,
    pub priority: TestPriority,
    pub tags: Vec<String>,
    pub setup: Option<TestStep>,
    pub steps: Vec<TestStep>,
    pub teardown: Option<TestStep>,
    pub assertions: Vec<TestAssertion>,
    pub timeout_seconds: Option<u64>,
    pub dependencies: Vec<String>,
    pub metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TestType {
    Unit,
    Integration,
    EndToEnd,
    Performance,
    Security,
    Determinism,
    Regression,
    Smoke,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TestPriority {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStep {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub action: TestAction,
    pub parameters: HashMap<String, Value>,
    pub timeout_seconds: Option<u64>,
    pub retries: Option<u32>,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestAction {
    ExecuteCommand {
        command: String,
        args: Vec<String>,
    },
    ApiCall {
        method: String,
        url: String,
        body: Option<Value>,
    },
    DatabaseOperation {
        operation: String,
        query: String,
        params: Vec<Value>,
    },
    FileOperation {
        operation: String,
        path: String,
        content: Option<String>,
    },
    NetworkOperation {
        operation: String,
        host: String,
        port: u16,
    },
    Custom {
        action_type: String,
        data: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestAssertion {
    pub id: String,
    pub name: String,
    pub assertion_type: AssertionType,
    pub parameters: HashMap<String, Value>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssertionType {
    Equals,
    NotEquals,
    GreaterThan,
    LessThan,
    Contains,
    NotContains,
    RegexMatch,
    FileExists,
    FileNotExists,
    DatabaseRecordExists,
    ApiResponse,
    Custom { assertion_type: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub test_case_id: String,
    pub status: TestStatus,
    pub execution_time_ms: u64,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub assertion_results: Vec<AssertionResult>,
    pub step_results: Vec<StepResult>,
    pub metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
    Error,
    Timeout,
}

impl TestStatus {
    pub fn severity(&self) -> u8 {
        match self {
            TestStatus::Passed => 0,
            TestStatus::Skipped => 1,
            TestStatus::Failed => 2,
            TestStatus::Timeout => 3,
            TestStatus::Error => 4,
        }
    }

    pub fn merge(&self, other: &TestStatus) -> TestStatus {
        if other.severity() >= self.severity() {
            *other
        } else {
            *self
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionResult {
    pub assertion_id: String,
    pub status: TestStatus,
    pub message: Option<String>,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: String,
    pub status: TestStatus,
    pub output: Option<String>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub test_cases: Vec<TestCase>,
    pub config: TestConfig,
    pub metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuiteResult {
    pub suite_id: String,
    pub status: TestStatus,
    pub execution_time_ms: u64,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub test_results: Vec<TestResult>,
    pub summary: TestSummary,
    pub metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    pub total_tests: u32,
    pub passed_tests: u32,
    pub failed_tests: u32,
    pub skipped_tests: u32,
    pub error_tests: u32,
    pub timeout_tests: u32,
    pub success_rate: f64,
    pub average_execution_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    pub overall_coverage: f64,
    pub line_coverage: f64,
    pub branch_coverage: f64,
    pub function_coverage: f64,
    pub file_coverage: HashMap<String, FileCoverage>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCoverage {
    pub file_path: String,
    pub line_coverage: f64,
    pub branch_coverage: f64,
    pub function_coverage: f64,
    pub covered_lines: Vec<u32>,
    pub uncovered_lines: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_execution_time_ms: u64,
    pub average_test_execution_time_ms: f64,
    pub slowest_test_execution_time_ms: u64,
    pub fastest_test_execution_time_ms: u64,
    pub memory_usage_bytes: u64,
    pub cpu_usage_percentage: f64,
    pub test_throughput: f64,
    pub timestamp: DateTime<Utc>,
}
