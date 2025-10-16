use crate::types::*;
use adapteros_core::{error::AosError, Result};
use regex::Regex;
use serde_json::Value;
use tokio::fs;
use tracing::debug;

macro_rules! json_value {
    ($($key:expr => $value:expr),+ $(,)?) => {{
        let mut map = serde_json::Map::new();
        $(map.insert($key.to_string(), $value);)+
        Value::Object(map)
    }};
}

pub async fn evaluate_assertion(assertion: &TestAssertion) -> Result<AssertionResult> {
    debug!(assertion_id = %assertion.id, assertion_name = %assertion.name, "running assertion");
    let mut status = TestStatus::Passed;
    let mut message = assertion.message.clone();
    let mut details = None;
    let params = &assertion.parameters;
    let get_required = |key: &str| -> Result<&Value> {
        params
            .get(key)
            .ok_or_else(|| AosError::Config(format!("missing assertion parameter: {}", key)))
    };
    match &assertion.assertion_type {
        AssertionType::Equals => {
            let left = get_required("left")?;
            let right = get_required("right")?;
            if left != right {
                status = TestStatus::Failed;
                message =
                    Some(message.unwrap_or_else(|| format!("expected {} but got {}", right, left)));
                details = Some(json_value!("left" => left.clone(), "right" => right.clone()));
            }
        }
        AssertionType::NotEquals => {
            let left = get_required("left")?;
            let right = get_required("right")?;
            if left == right {
                status = TestStatus::Failed;
                message = Some(message.unwrap_or_else(|| "values should differ".into()));
                details = Some(json_value!("value" => left.clone()));
            }
        }
        AssertionType::GreaterThan => {
            let left = get_required("left")?
                .as_f64()
                .ok_or_else(|| AosError::Config("left must be numeric".into()))?;
            let right = get_required("right")?
                .as_f64()
                .ok_or_else(|| AosError::Config("right must be numeric".into()))?;
            if left <= right {
                status = TestStatus::Failed;
                message = Some(
                    message.unwrap_or_else(|| format!("{} is not greater than {}", left, right)),
                );
            }
        }
        AssertionType::LessThan => {
            let left = get_required("left")?
                .as_f64()
                .ok_or_else(|| AosError::Config("left must be numeric".into()))?;
            let right = get_required("right")?
                .as_f64()
                .ok_or_else(|| AosError::Config("right must be numeric".into()))?;
            if left >= right {
                status = TestStatus::Failed;
                message =
                    Some(message.unwrap_or_else(|| format!("{} is not less than {}", left, right)));
            }
        }
        AssertionType::Contains => {
            let container = get_required("container")?
                .as_str()
                .ok_or_else(|| AosError::Config("container must be string".into()))?;
            let needle = get_required("item")?
                .as_str()
                .ok_or_else(|| AosError::Config("item must be string".into()))?;
            if !container.contains(needle) {
                status = TestStatus::Failed;
                message = Some(
                    message.unwrap_or_else(|| format!("{} does not contain {}", container, needle)),
                );
            }
        }
        AssertionType::NotContains => {
            let container = get_required("container")?
                .as_str()
                .ok_or_else(|| AosError::Config("container must be string".into()))?;
            let needle = get_required("item")?
                .as_str()
                .ok_or_else(|| AosError::Config("item must be string".into()))?;
            if container.contains(needle) {
                status = TestStatus::Failed;
                message =
                    Some(message.unwrap_or_else(|| {
                        format!("{} unexpectedly contains {}", container, needle)
                    }));
            }
        }
        AssertionType::RegexMatch => {
            let pattern = get_required("pattern")?
                .as_str()
                .ok_or_else(|| AosError::Config("pattern must be string".into()))?;
            let value = get_required("value")?
                .as_str()
                .ok_or_else(|| AosError::Config("value must be string".into()))?;
            let regex = Regex::new(pattern).map_err(|err| AosError::Config(err.to_string()))?;
            if !regex.is_match(value) {
                status = TestStatus::Failed;
                message = Some(
                    message.unwrap_or_else(|| format!("{} does not match {}", value, pattern)),
                );
            }
        }
        AssertionType::FileExists => {
            let path = get_required("path")?
                .as_str()
                .ok_or_else(|| AosError::Config("path must be string".into()))?;
            if fs::metadata(path).await.is_err() {
                status = TestStatus::Failed;
                message = Some(message.unwrap_or_else(|| format!("file {} does not exist", path)));
            }
        }
        AssertionType::FileNotExists => {
            let path = get_required("path")?
                .as_str()
                .ok_or_else(|| AosError::Config("path must be string".into()))?;
            if fs::metadata(path).await.is_ok() {
                status = TestStatus::Failed;
                message =
                    Some(message.unwrap_or_else(|| format!("file {} unexpectedly exists", path)));
            }
        }
        AssertionType::DatabaseRecordExists => {
            let exists = get_required("exists")?
                .as_bool()
                .ok_or_else(|| AosError::Config("exists must be bool".into()))?;
            if !exists {
                status = TestStatus::Failed;
                message = Some(message.unwrap_or_else(|| "database record not found".into()));
            }
        }
        AssertionType::ApiResponse => {
            let status_code = get_required("status")?
                .as_u64()
                .ok_or_else(|| AosError::Config("status must be number".into()))?;
            let expected = get_required("expected_status")?
                .as_u64()
                .ok_or_else(|| AosError::Config("expected_status must be number".into()))?;
            if status_code != expected {
                status = TestStatus::Failed;
                message =
                    Some(message.unwrap_or_else(|| {
                        format!("expected {} but got {}", expected, status_code)
                    }));
            }
        }
        AssertionType::Custom { assertion_type } => {
            if params
                .get("passed")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            {
                details =
                    Some(json_value!("assertion_type" => Value::from(assertion_type.clone())));
            } else {
                status = TestStatus::Failed;
                message = Some(
                    message
                        .unwrap_or_else(|| format!("custom assertion {} failed", assertion_type)),
                );
            }
        }
    }
    debug!(assertion_id = %assertion.id, status = ?status, "assertion completed");
    Ok(AssertionResult {
        assertion_id: assertion.id.clone(),
        status,
        message,
        details,
    })
}
