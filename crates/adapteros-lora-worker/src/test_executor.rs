//! Test execution framework for patch validation
//!
//! Provides automated test execution for Rust projects using cargo test or cargo-nextest.
//! Parses test output and extracts structured results for validation pipeline.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Test executor for running cargo tests
#[derive(Debug, Clone)]
pub struct TestExecutor {
    repo_path: PathBuf,
    timeout_secs: u64,
}

/// Test execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub framework: TestFramework,
    pub passed: usize,
    pub failed: usize,
    pub ignored: usize,
    pub duration_ms: u64,
    pub failures: Vec<TestFailure>,
}

/// Test framework type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TestFramework {
    CargoTest,
    CargoNextest,
}

/// Individual test failure details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFailure {
    pub test_name: String,
    pub message: String,
    pub location: Option<String>,
}

impl TestExecutor {
    /// Create a new test executor for the given repository
    pub fn new<P: AsRef<Path>>(repo_path: P) -> Self {
        Self {
            repo_path: repo_path.as_ref().to_path_buf(),
            timeout_secs: 300, // 5 minutes default
        }
    }

    /// Set timeout for test execution
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Execute tests and return structured results
    pub async fn run_tests(&self) -> Result<TestResult> {
        // Detect which test framework to use
        let framework = self.detect_framework()?;

        info!(
            "Running tests with {:?} in {}",
            framework,
            self.repo_path.display()
        );

        let start = Instant::now();
        let output = self.execute_tests(&framework).await?;
        let duration_ms = start.elapsed().as_millis() as u64;

        // Parse test output
        let result = self.parse_test_output(&output, framework, duration_ms)?;

        info!(
            "Test execution complete: {} passed, {} failed, {} ignored",
            result.passed, result.failed, result.ignored
        );

        Ok(result)
    }

    /// Detect which test framework is available
    fn detect_framework(&self) -> Result<TestFramework> {
        // Check if cargo-nextest is installed
        let nextest_check = Command::new("cargo")
            .arg("nextest")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        match nextest_check {
            Ok(status) if status.success() => {
                debug!("Detected cargo-nextest");
                Ok(TestFramework::CargoNextest)
            }
            _ => {
                debug!("Using cargo test (nextest not available)");
                Ok(TestFramework::CargoTest)
            }
        }
    }

    /// Execute tests using the specified framework
    async fn execute_tests(&self, framework: &TestFramework) -> Result<String> {
        let mut cmd = Command::new("cargo");

        match framework {
            TestFramework::CargoTest => {
                cmd.arg("test")
                    .arg("--")
                    .arg("--nocapture")
                    .arg("--test-threads=1");
            }
            TestFramework::CargoNextest => {
                cmd.arg("nextest").arg("run").arg("--no-capture");
            }
        }

        cmd.current_dir(&self.repo_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("Executing: {:?}", cmd);

        // Execute with timeout
        let child = cmd
            .spawn()
            .map_err(|e| AosError::Worker(format!("Failed to spawn test process: {}", e)))?;

        // Use tokio to handle timeout
        let output = tokio::time::timeout(
            Duration::from_secs(self.timeout_secs),
            tokio::task::spawn_blocking(move || child.wait_with_output()),
        )
        .await
        .map_err(|_| AosError::Worker("Test execution timeout".to_string()))?
        .map_err(|e| AosError::Worker(format!("Failed to wait for test process: {}", e)))?
        .map_err(|e| AosError::Worker(format!("Test process error: {}", e)))?;

        // Combine stdout and stderr
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        Ok(combined)
    }

    /// Parse test output into structured result
    fn parse_test_output(
        &self,
        output: &str,
        framework: TestFramework,
        duration_ms: u64,
    ) -> Result<TestResult> {
        match framework {
            TestFramework::CargoTest => self.parse_cargo_test_output(output, duration_ms),
            TestFramework::CargoNextest => self.parse_nextest_output(output, duration_ms),
        }
    }

    /// Parse cargo test output
    fn parse_cargo_test_output(&self, output: &str, duration_ms: u64) -> Result<TestResult> {
        let mut passed = 0;
        let mut failed = 0;
        let mut ignored = 0;
        let mut failures = Vec::new();

        // Parse test result summary line
        // Example: "test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
        if let Some(summary_line) = output.lines().find(|line| line.contains("test result:")) {
            if let Some(stats) = summary_line.split("test result:").nth(1) {
                // Extract numbers using regex-like parsing
                for part in stats.split(';') {
                    let part = part.trim();
                    if part.contains("passed") {
                        passed = self.extract_number(part).unwrap_or(0);
                    } else if part.contains("failed") {
                        failed = self.extract_number(part).unwrap_or(0);
                    } else if part.contains("ignored") {
                        ignored = self.extract_number(part).unwrap_or(0);
                    }
                }
            }
        }

        // Parse individual test failures
        let mut current_test: Option<String> = None;
        let mut failure_message = String::new();

        for line in output.lines() {
            // Detect test failure: "test path::to::test ... FAILED"
            if line.contains("... FAILED") {
                if let Some(test_name) = line.split_whitespace().nth(1) {
                    current_test = Some(test_name.to_string());
                    failure_message.clear();
                }
            }
            // Collect failure details
            else if current_test.is_some()
                && (line.starts_with("thread") || line.contains("panicked"))
            {
                failure_message.push_str(line);
                failure_message.push('\n');
            }
            // End of failure block
            else if current_test.is_some() && line.trim().is_empty() {
                if let Some(test_name) = current_test.take() {
                    failures.push(TestFailure {
                        test_name,
                        message: failure_message.trim().to_string(),
                        location: None,
                    });
                    failure_message.clear();
                }
            }
        }

        Ok(TestResult {
            framework: TestFramework::CargoTest,
            passed,
            failed,
            ignored,
            duration_ms,
            failures,
        })
    }

    /// Parse cargo-nextest output
    fn parse_nextest_output(&self, output: &str, duration_ms: u64) -> Result<TestResult> {
        let mut passed = 0;
        let mut failed = 0;
        let mut ignored = 0;
        let mut failures = Vec::new();

        // Nextest uses structured output
        // Example: "  PASS [   0.010s] crate::module::test_name"
        // Example: "  FAIL [   0.005s] crate::module::test_name"

        for line in output.lines() {
            let line = line.trim();

            if line.starts_with("PASS") {
                passed += 1;
            } else if line.starts_with("FAIL") {
                failed += 1;
                // Extract test name
                if let Some(test_name) = line.split(']').nth(1) {
                    failures.push(TestFailure {
                        test_name: test_name.trim().to_string(),
                        message: "Test failed (see nextest output for details)".to_string(),
                        location: None,
                    });
                }
            } else if line.starts_with("SKIP") {
                ignored += 1;
            }
        }

        // If we couldn't parse individual results, try summary line
        if passed == 0 && failed == 0 {
            // Look for summary: "Summary: 15 passed, 0 failed, 0 skipped"
            if let Some(summary) = output.lines().find(|line| line.contains("Summary:")) {
                for part in summary.split(',') {
                    if part.contains("passed") {
                        passed = self.extract_number(part).unwrap_or(0);
                    } else if part.contains("failed") {
                        failed = self.extract_number(part).unwrap_or(0);
                    } else if part.contains("skipped") {
                        ignored = self.extract_number(part).unwrap_or(0);
                    }
                }
            }
        }

        Ok(TestResult {
            framework: TestFramework::CargoNextest,
            passed,
            failed,
            ignored,
            duration_ms,
            failures,
        })
    }

    /// Extract a number from a string like "15 passed"
    fn extract_number(&self, s: &str) -> Option<usize> {
        s.split_whitespace()
            .find_map(|word| word.parse::<usize>().ok())
    }

    /// Check if the repository has tests configured
    pub fn has_tests(&self) -> bool {
        // Check for Cargo.toml with test configuration
        let cargo_toml = self.repo_path.join("Cargo.toml");
        if !cargo_toml.exists() {
            return false;
        }

        // Check for tests directory or test files
        let tests_dir = self.repo_path.join("tests");
        if tests_dir.exists() && tests_dir.is_dir() {
            return true;
        }

        // Check for inline tests in src
        let src_dir = self.repo_path.join("src");
        if src_dir.exists() && src_dir.is_dir() {
            return true;
        }

        false
    }
}

impl TestResult {
    /// Check if all tests passed
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }

    /// Get total number of tests run
    pub fn total_tests(&self) -> usize {
        self.passed + self.failed + self.ignored
    }

    /// Calculate success rate
    pub fn success_rate(&self) -> f32 {
        if self.total_tests() == 0 {
            return 1.0;
        }
        self.passed as f32 / (self.passed + self.failed) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cargo_test_summary() {
        let output = "test result: ok. 15 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out";
        let executor = TestExecutor::new(".");
        let result = executor.parse_cargo_test_output(output, 1000).unwrap();

        assert_eq!(result.passed, 15);
        assert_eq!(result.failed, 0);
        assert_eq!(result.ignored, 2);
        assert!(result.all_passed());
    }

    #[test]
    fn test_parse_cargo_test_with_failures() {
        let output = r#"
running 3 tests
test test_one ... ok
test test_two ... FAILED
test test_three ... ok

failures:

---- test_two stdout ----
thread 'test_two' panicked at 'assertion failed: false'

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
"#;
        let executor = TestExecutor::new(".");
        let result = executor.parse_cargo_test_output(output, 1000).unwrap();

        assert_eq!(result.passed, 2);
        assert_eq!(result.failed, 1);
        assert_eq!(result.failures.len(), 1);
        assert!(!result.all_passed());
    }

    #[test]
    fn test_success_rate_calculation() {
        let result = TestResult {
            framework: TestFramework::CargoTest,
            passed: 8,
            failed: 2,
            ignored: 0,
            duration_ms: 1000,
            failures: vec![],
        };

        assert_eq!(result.success_rate(), 0.8);
        assert_eq!(result.total_tests(), 10);
    }
}
