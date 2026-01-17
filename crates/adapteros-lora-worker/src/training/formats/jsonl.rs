//! JSONL format parser for Plan 4 training datasets.
//!
//! Accepted schemas (per-line JSON object only):
//! - Supervised: {"prompt": "...", "completion": "..."}
//! - Raw text: {"text": "..."}

use super::RawSample;
use adapteros_core::{AosError, B3Hash, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

const SUPERVISED_PROMPT_KEY: &str = "prompt";
const SUPERVISED_COMPLETION_KEY: &str = "completion";
const RAW_TEXT_KEY: &str = "text";
const SCHEMA_SUPERVISED: &str = "supervised";
const SCHEMA_RAW_CONTINUATION: &str = "raw_continuation_v1";

/// Parse a JSONL file into raw samples.
pub fn parse_jsonl_file(path: &Path) -> Result<Vec<RawSample>> {
    let file = File::open(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to open JSONL file {}: {}",
            path.display(),
            e
        ))
    })?;
    let reader = BufReader::new(file);
    let path_str = path.display().to_string();

    let mut samples = Vec::new();
    let mut schema_mode: Option<String> = None;

    for (line_idx, line_result) in reader.lines().enumerate() {
        let line_num = line_idx + 1;
        let line = line_result.map_err(|e| {
            AosError::Io(format!(
                "Failed to read line {} in {}: {}",
                line_num, path_str, e
            ))
        })?;

        if line.trim().is_empty() {
            return Err(AosError::Validation(format!(
                "Empty JSONL line at {}:{}",
                path_str, line_num
            )));
        }

        let sample = parse_jsonl_line(&line, &path_str, line_num)?;
        let schema = sample.metadata.get("schema").cloned().ok_or_else(|| {
            AosError::Validation(format!(
                "Missing schema in parsed JSONL sample at {}:{}",
                path_str, line_num
            ))
        })?;
        if let Some(active) = schema_mode.as_ref() {
            if active != &schema {
                return Err(AosError::Validation(format!(
                    "Mixed JSONL schemas in {}: expected {}, found {} at line {}",
                    path_str, active, schema, line_num
                )));
            }
        } else {
            schema_mode = Some(schema);
        }
        samples.push(sample);
    }

    if samples.is_empty() {
        return Err(AosError::Validation(format!(
            "JSONL file {} contains no valid samples",
            path_str
        )));
    }

    Ok(samples)
}

/// Parse a single JSONL line into a raw sample.
fn parse_jsonl_line(line: &str, path: &str, line_num: usize) -> Result<RawSample> {
    let context = format!("{}:{}", path, line_num);

    let obj: Value = serde_json::from_str(line)
        .map_err(|e| AosError::Validation(format!("Invalid JSON at {}: {}", context, e)))?;

    let obj = obj.as_object().ok_or_else(|| {
        AosError::Validation(format!("Expected JSON object at {}, got {}", context, obj))
    })?;

    let is_supervised = obj.len() == 2
        && obj.contains_key(SUPERVISED_PROMPT_KEY)
        && obj.contains_key(SUPERVISED_COMPLETION_KEY);
    let is_raw = obj.len() == 1 && obj.contains_key(RAW_TEXT_KEY);

    if !is_supervised && !is_raw {
        return Err(AosError::Validation(format!(
            "Unsupported JSONL schema at {}. Expected {{\"prompt\",\"completion\"}} or {{\"text\"}} only",
            context
        )));
    }

    let source_hash = B3Hash::hash(line.as_bytes()).to_hex();

    let mut metadata = HashMap::new();
    metadata.insert("source_file".to_string(), path.to_string());
    metadata.insert("source_line".to_string(), line_num.to_string());
    metadata.insert("source_hash".to_string(), source_hash);
    metadata.insert("row_id".to_string(), line_num.to_string());

    if is_supervised {
        let prompt = obj
            .get(SUPERVISED_PROMPT_KEY)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AosError::Validation(format!("Line {} has empty prompt", context))
            })?;
        let completion = obj
            .get(SUPERVISED_COMPLETION_KEY)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AosError::Validation(format!("Line {} has empty completion", context))
            })?;
        metadata.insert("schema".to_string(), SCHEMA_SUPERVISED.to_string());
        return Ok(RawSample {
            input: prompt.to_string(),
            target: completion.to_string(),
            weight: 1.0,
            metadata,
        });
    }

    let text = obj
        .get(RAW_TEXT_KEY)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AosError::Validation(format!("Line {} has empty text", context)))?;
    metadata.insert(
        "schema".to_string(),
        SCHEMA_RAW_CONTINUATION.to_string(),
    );
    Ok(RawSample {
        input: text.to_string(),
        target: String::new(),
        weight: 1.0,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_jsonl(lines: &[&str]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(file, "{}", line).unwrap();
        }
        file
    }

    #[test]
    fn test_parse_prompt_completion() {
        let file = write_temp_jsonl(&[r#"{"prompt": "Hello", "completion": "World"}"#]);
        let samples = parse_jsonl_file(file.path()).unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].input, "Hello");
        assert_eq!(samples[0].target, "World");
    }

    #[test]
    fn test_parse_raw_text() {
        let file = write_temp_jsonl(&[r#"{"text": "Hello world"}"#]);
        let samples = parse_jsonl_file(file.path()).unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].input, "Hello world");
    }

    #[test]
    fn test_empty_line_is_rejected() {
        let file = write_temp_jsonl(&[
            r#"{"prompt": "a", "completion": "b"}"#,
            "",
        ]);
        let err = parse_jsonl_file(file.path()).unwrap_err();
        assert!(err.to_string().contains("Empty JSONL line"));
    }

    #[test]
    fn test_rejects_extra_fields() {
        let file =
            write_temp_jsonl(&[r#"{"prompt": "a", "completion": "b", "extra": "nope"}"#]);
        let err = parse_jsonl_file(file.path()).unwrap_err();
        assert!(err.to_string().contains("Unsupported JSONL schema"));
    }

    #[test]
    fn test_rejects_missing_fields() {
        let file = write_temp_jsonl(&[r#"{"prompt": "a"}"#]);
        let err = parse_jsonl_file(file.path()).unwrap_err();
        assert!(err.to_string().contains("Unsupported JSONL schema"));
    }

    #[test]
    fn test_rejects_mixed_schema() {
        let file = write_temp_jsonl(&[
            r#"{"prompt": "a", "completion": "b"}"#,
            r#"{"text": "c"}"#,
        ]);
        let err = parse_jsonl_file(file.path()).unwrap_err();
        assert!(err.to_string().contains("Mixed JSONL schemas"));
    }

    #[test]
    fn test_invalid_json() {
        let file = write_temp_jsonl(&[r#"{"prompt": "a", "completion": "b"}"#, r#"not json"#]);
        let err = parse_jsonl_file(file.path()).unwrap_err();
        assert!(err.to_string().contains("Invalid JSON"));
    }
}
