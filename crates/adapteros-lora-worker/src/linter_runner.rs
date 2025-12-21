//! Linter integration framework for patch validation
//!
//! Provides automated linter execution for Rust projects using clippy and rustfmt.
//! Parses linter output and extracts structured issues for validation pipeline.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::{debug, info};

/// Linter runner for executing clippy and rustfmt
#[derive(Debug, Clone)]
pub struct LinterRunner {
    repo_path: PathBuf,
    config: LinterConfig,
}

/// Linter configuration
#[derive(Debug, Clone)]
pub struct LinterConfig {
    pub run_clippy: bool,
    pub run_rustfmt: bool,
    pub clippy_args: Vec<String>,
    pub fail_on_warnings: bool,
}

/// Linter execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinterResult {
    pub linter: LinterType,
    pub errors: Vec<LintIssue>,
    pub warnings: Vec<LintIssue>,
    pub duration_ms: u64,
}

/// Type of linter
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinterType {
    Clippy,
    Rustfmt,
}

/// Individual lint issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintIssue {
    pub file_path: String,
    pub line: usize,
    pub column: Option<usize>,
    pub severity: LintSeverity,
    pub message: String,
    pub code: Option<String>,
}

/// Severity of lint issue
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LintSeverity {
    Error,
    Warning,
    Info,
    Help,
}

impl Default for LinterConfig {
    fn default() -> Self {
        Self {
            run_clippy: true,
            run_rustfmt: true,
            clippy_args: vec!["-D".to_string(), "warnings".to_string()],
            fail_on_warnings: false,
        }
    }
}

impl LinterRunner {
    /// Create a new linter runner for the given repository
    pub fn new<P: AsRef<Path>>(repo_path: P) -> Self {
        Self {
            repo_path: repo_path.as_ref().to_path_buf(),
            config: LinterConfig::default(),
        }
    }

    /// Set linter configuration
    pub fn with_config(mut self, config: LinterConfig) -> Self {
        self.config = config;
        self
    }

    /// Run all configured linters
    pub async fn run_linters(&self) -> Result<Vec<LinterResult>> {
        let mut results = Vec::new();

        if self.config.run_clippy {
            info!("Running clippy in {}", self.repo_path.display());
            if let Ok(result) = self.run_clippy().await {
                results.push(result);
            }
        }

        if self.config.run_rustfmt {
            info!("Running rustfmt in {}", self.repo_path.display());
            if let Ok(result) = self.run_rustfmt().await {
                results.push(result);
            }
        }

        Ok(results)
    }

    /// Run clippy and parse output
    async fn run_clippy(&self) -> Result<LinterResult> {
        let start = std::time::Instant::now();

        let mut cmd = Command::new("cargo");
        cmd.arg("clippy")
            .arg("--message-format=json")
            .arg("--all-targets")
            .arg("--")
            .args(&self.config.clippy_args);

        cmd.current_dir(&self.repo_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("Executing: {:?}", cmd);

        let output = cmd
            .output()
            .map_err(|e| AosError::Worker(format!("Failed to run clippy: {}", e)))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Parse JSON output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let issues = self.parse_clippy_json(&stdout)?;

        // Separate errors and warnings
        let (errors, warnings): (Vec<_>, Vec<_>) = issues
            .into_iter()
            .partition(|issue| issue.severity == LintSeverity::Error);

        info!(
            "Clippy complete: {} errors, {} warnings",
            errors.len(),
            warnings.len()
        );

        Ok(LinterResult {
            linter: LinterType::Clippy,
            errors,
            warnings,
            duration_ms,
        })
    }

    /// Parse clippy JSON output
    fn parse_clippy_json(&self, output: &str) -> Result<Vec<LintIssue>> {
        let mut issues = Vec::new();

        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }

            // Try to parse as JSON message
            if let Ok(msg) = serde_json::from_str::<ClippyMessage>(line) {
                if msg.reason == "compiler-message" {
                    if let Some(issue) = self.extract_clippy_issue(&msg) {
                        issues.push(issue);
                    }
                }
            }
        }

        Ok(issues)
    }

    /// Extract lint issue from clippy message
    fn extract_clippy_issue(&self, msg: &ClippyMessage) -> Option<LintIssue> {
        let message = &msg.message;

        // Get primary span
        let span = message.spans.first()?;

        // Determine severity
        let severity = match message.level.as_str() {
            "error" => LintSeverity::Error,
            "warning" => LintSeverity::Warning,
            "note" | "help" => LintSeverity::Help,
            _ => LintSeverity::Info,
        };

        Some(LintIssue {
            file_path: span.file_name.clone(),
            line: span.line_start,
            column: Some(span.column_start),
            severity,
            message: message.message.clone(),
            code: message.code.as_ref().map(|c| c.code.clone()),
        })
    }

    /// Run rustfmt check
    async fn run_rustfmt(&self) -> Result<LinterResult> {
        let start = std::time::Instant::now();

        let mut cmd = Command::new("cargo");
        cmd.arg("fmt").arg("--check").arg("--message-format=short");

        cmd.current_dir(&self.repo_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("Executing: {:?}", cmd);

        let output = cmd
            .output()
            .map_err(|e| AosError::Worker(format!("Failed to run rustfmt: {}", e)))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Parse output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        let issues = self.parse_rustfmt_output(&combined)?;

        info!("Rustfmt complete: {} formatting issues", issues.len());

        Ok(LinterResult {
            linter: LinterType::Rustfmt,
            errors: issues.clone(),
            warnings: vec![],
            duration_ms,
        })
    }

    /// Parse rustfmt output
    fn parse_rustfmt_output(&self, output: &str) -> Result<Vec<LintIssue>> {
        let mut issues = Vec::new();

        for line in output.lines() {
            // Rustfmt outputs: "Diff in /path/to/file.rs at line 42"
            if line.starts_with("Diff in") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let file_path = parts[2].to_string();
                    let line_num = if parts.len() >= 6 {
                        parts[5].parse::<usize>().unwrap_or(1)
                    } else {
                        1
                    };

                    issues.push(LintIssue {
                        file_path,
                        line: line_num,
                        column: None,
                        severity: LintSeverity::Error,
                        message: "Formatting issue detected".to_string(),
                        code: Some("rustfmt".to_string()),
                    });
                }
            }
        }

        Ok(issues)
    }

    /// Check if all linters passed
    pub fn all_passed(results: &[LinterResult]) -> bool {
        results.iter().all(|r| r.errors.is_empty())
    }

    /// Get total error count across all linters
    pub fn total_errors(results: &[LinterResult]) -> usize {
        results.iter().map(|r| r.errors.len()).sum()
    }

    /// Get total warning count across all linters
    pub fn total_warnings(results: &[LinterResult]) -> usize {
        results.iter().map(|r| r.warnings.len()).sum()
    }
}

/// Clippy JSON message structure
#[derive(Debug, Deserialize)]
struct ClippyMessage {
    reason: String,
    message: CompilerMessage,
}

/// Compiler message structure
#[derive(Debug, Deserialize)]
struct CompilerMessage {
    message: String,
    level: String,
    spans: Vec<Span>,
    code: Option<DiagnosticCode>,
}

/// Source span
#[derive(Debug, Deserialize)]
struct Span {
    file_name: String,
    line_start: usize,
    column_start: usize,
}

/// Diagnostic code
#[derive(Debug, Deserialize)]
struct DiagnosticCode {
    code: String,
}

impl LinterResult {
    /// Check if this linter passed (no errors)
    pub fn passed(&self) -> bool {
        self.errors.is_empty()
    }

    /// Get total issue count
    pub fn total_issues(&self) -> usize {
        self.errors.len() + self.warnings.len()
    }

    /// Get issues by severity
    pub fn issues_by_severity(&self, severity: LintSeverity) -> Vec<&LintIssue> {
        self.errors
            .iter()
            .chain(self.warnings.iter())
            .filter(|issue| issue.severity == severity)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rustfmt_output() {
        let output = r#"
Diff in /path/to/file.rs at line 42
Diff in /path/to/other.rs at line 10
"#;
        let runner = LinterRunner::new(".");
        let issues = runner.parse_rustfmt_output(output).unwrap();

        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].line, 42);
        assert_eq!(issues[1].line, 10);
    }

    #[test]
    fn test_linter_result_passed() {
        let result = LinterResult {
            linter: LinterType::Clippy,
            errors: vec![],
            warnings: vec![],
            duration_ms: 1000,
        };

        assert!(result.passed());
        assert_eq!(result.total_issues(), 0);
    }

    #[test]
    fn test_total_errors_warnings() {
        let results = vec![
            LinterResult {
                linter: LinterType::Clippy,
                errors: vec![LintIssue {
                    file_path: "test.rs".to_string(),
                    line: 1,
                    column: Some(1),
                    severity: LintSeverity::Error,
                    message: "error".to_string(),
                    code: None,
                }],
                warnings: vec![],
                duration_ms: 1000,
            },
            LinterResult {
                linter: LinterType::Rustfmt,
                errors: vec![],
                warnings: vec![],
                duration_ms: 500,
            },
        ];

        assert_eq!(LinterRunner::total_errors(&results), 1);
        assert_eq!(LinterRunner::total_warnings(&results), 0);
        assert!(!LinterRunner::all_passed(&results));
    }
}
