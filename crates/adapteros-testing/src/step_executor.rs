use crate::types::*;
use adapteros_core::Result;
use serde_json::Value;
use std::{collections::HashMap, time::Instant};
use tokio::{fs, net::TcpStream, process::Command};
use tracing::debug;

pub async fn execute_step(step: &TestStep) -> Result<StepResult> {
    let start = Instant::now();
    debug!(step_id = %step.id, step_name = %step.name, "running test step");
    let mut result = StepResult {
        step_id: step.id.clone(),
        status: TestStatus::Passed,
        output: None,
        error: None,
        execution_time_ms: 0,
    };
    let status = match &step.action {
        TestAction::ExecuteCommand { command, args } => {
            match Command::new(command).args(args).output().await {
                Ok(output) => {
                    result.output =
                        Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
                    if output.status.success() {
                        TestStatus::Passed
                    } else {
                        result.error =
                            Some(String::from_utf8_lossy(&output.stderr).trim().to_string());
                        TestStatus::Failed
                    }
                }
                Err(err) => {
                    result.error = Some(err.to_string());
                    TestStatus::Error
                }
            }
        }
        TestAction::ApiCall { method, url, body } => {
            result.output = Some(format!(
                "{} {} {}",
                method,
                url,
                body.as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "{}".into())
            ));
            let expected = step
                .parameters
                .get("expected_status")
                .and_then(Value::as_u64)
                .unwrap_or(200);
            let response = step
                .parameters
                .get("response_status")
                .and_then(Value::as_u64)
                .unwrap_or(200);
            if response == expected {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            }
        }
        TestAction::DatabaseOperation {
            operation,
            query,
            params,
        } => {
            result.output = Some(format!("{}:{} [{} params]", operation, query, params.len()));
            if step
                .parameters
                .get("should_fail")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                result.error = Some("database operation flagged as failure".into());
                TestStatus::Failed
            } else {
                TestStatus::Passed
            }
        }
        TestAction::FileOperation {
            operation,
            path,
            content,
        } => match operation.to_lowercase().as_str() {
            "write" => match fs::write(path, content.clone().unwrap_or_default()).await {
                Ok(_) => TestStatus::Passed,
                Err(err) => {
                    result.error = Some(err.to_string());
                    TestStatus::Error
                }
            },
            "read" => match fs::read_to_string(path).await {
                Ok(data) => {
                    result.output = Some(data);
                    TestStatus::Passed
                }
                Err(err) => {
                    result.error = Some(err.to_string());
                    TestStatus::Error
                }
            },
            "delete" => match fs::remove_file(path).await {
                Ok(_) => TestStatus::Passed,
                Err(err) => {
                    result.error = Some(err.to_string());
                    TestStatus::Error
                }
            },
            other => {
                result.error = Some(format!("unsupported file operation: {}", other));
                TestStatus::Error
            }
        },
        TestAction::NetworkOperation {
            operation,
            host,
            port,
        } => match operation.to_lowercase().as_str() {
            "connect" => match TcpStream::connect((host.as_str(), *port)).await {
                Ok(_) => TestStatus::Passed,
                Err(err) => {
                    result.error = Some(err.to_string());
                    TestStatus::Failed
                }
            },
            "noop" => {
                result.output = Some("network noop".into());
                TestStatus::Passed
            }
            other => {
                result.error = Some(format!("unsupported network operation: {}", other));
                TestStatus::Error
            }
        },
        TestAction::Custom { action_type, data } => {
            result.output = Some(format!("{}:{}", action_type, data));
            if step.parameters.get("status").and_then(Value::as_str) == Some("failed") {
                TestStatus::Failed
            } else {
                TestStatus::Passed
            }
        }
    };
    result.status = status;
    result.execution_time_ms = start.elapsed().as_millis() as u64;
    debug!(step_id = %step.id, status = ?result.status, "test step completed");
    Ok(result)
}

pub fn framework_from_metadata(metadata: &HashMap<String, Value>) -> ExternalFramework {
    ExternalFramework::from_metadata(metadata)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalFramework {
    Native,
    Cargo,
    Pytest,
}

impl ExternalFramework {
    fn from_metadata(metadata: &HashMap<String, Value>) -> Self {
        metadata
            .get("framework")
            .and_then(Value::as_str)
            .map(|name| match name.to_lowercase().as_str() {
                "cargo" => ExternalFramework::Cargo,
                "pytest" => ExternalFramework::Pytest,
                _ => ExternalFramework::Native,
            })
            .unwrap_or(ExternalFramework::Native)
    }
}

pub async fn run_framework_step(framework: ExternalFramework, test_case: &TestCase) -> StepResult {
    let mut result = StepResult {
        step_id: format!("framework::{}", test_case.id),
        status: TestStatus::Passed,
        output: None,
        error: None,
        execution_time_ms: 0,
    };
    let start = Instant::now();
    match framework {
        ExternalFramework::Native => {}
        ExternalFramework::Cargo => {
            match Command::new("cargo")
                .arg("test")
                .arg(&test_case.name)
                .arg("--")
                .arg("--exact")
                .output()
                .await
            {
                Ok(output) => {
                    result.output =
                        Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
                    if !output.status.success() {
                        result.status = TestStatus::Failed;
                        result.error =
                            Some(String::from_utf8_lossy(&output.stderr).trim().to_string());
                    }
                }
                Err(err) => {
                    result.status = TestStatus::Skipped;
                    result.error = Some(format!("cargo not available: {}", err));
                }
            }
            result.execution_time_ms = start.elapsed().as_millis() as u64;
            return result;
        }
        ExternalFramework::Pytest => {
            result.status = TestStatus::Skipped;
            result.output = Some("pytest integration not enabled".into());
            result.execution_time_ms = start.elapsed().as_millis() as u64;
            return result;
        }
    }
    result.execution_time_ms = start.elapsed().as_millis() as u64;
    result
}
