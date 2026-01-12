//! JSONL format parser for instruction-tuning datasets.
//!
//! Supports common field names:
//! - Input: instruction, input, prompt, question
//! - Target: output, response, answer, completion

use super::RawSample;
use crate::training::normalize::{normalize_text, validate_non_empty};
use adapteros_core::{AosError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Input field names in priority order.
const INPUT_FIELDS: &[&str] = &["instruction", "input", "prompt", "question"];

/// Target field names in priority order.
const TARGET_FIELDS: &[&str] = &["output", "response", "answer", "completion"];

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

    for (line_idx, line_result) in reader.lines().enumerate() {
        let line_num = line_idx + 1;
        let line = line_result.map_err(|e| {
            AosError::Io(format!(
                "Failed to read line {} in {}: {}",
                line_num, path_str, e
            ))
        })?;

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        let sample = parse_jsonl_line(&line, &path_str, line_num)?;
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

    // Find input field
    let input_raw = find_field(obj, INPUT_FIELDS).ok_or_else(|| {
        AosError::Validation(format!(
            "Missing input field at {}. Expected one of: {:?}",
            context, INPUT_FIELDS
        ))
    })?;

    // Find target field
    let target_raw = find_field(obj, TARGET_FIELDS).ok_or_else(|| {
        AosError::Validation(format!(
            "Missing target field at {}. Expected one of: {:?}",
            context, TARGET_FIELDS
        ))
    })?;

    // Normalize text
    let input = normalize_text(&input_raw)?;
    let target = normalize_text(&target_raw)?;

    // Validate non-empty
    validate_non_empty(&input, "input", &context)?;
    validate_non_empty(&target, "target", &context)?;

    // Extract weight if present (must be non-negative)
    let weight = obj
        .get("weight")
        .and_then(|v| v.as_f64())
        .map(|w| w as f32)
        .unwrap_or(1.0);

    if weight < 0.0 {
        return Err(AosError::Validation(format!(
            "Negative weight {} at {}. Weights must be >= 0.0",
            weight, context
        )));
    }

    // Extract metadata
    let mut metadata = HashMap::new();
    metadata.insert("source_file".to_string(), path.to_string());
    metadata.insert("source_line".to_string(), line_num.to_string());

    if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
        metadata.insert("id".to_string(), id.to_string());
    }

    // Copy any additional string fields as metadata
    for (key, value) in obj {
        if !INPUT_FIELDS.contains(&key.as_str())
            && !TARGET_FIELDS.contains(&key.as_str())
            && key != "weight"
            && key != "id"
        {
            if let Some(s) = value.as_str() {
                metadata.insert(key.clone(), s.to_string());
            }
        }
    }

    Ok(RawSample {
        input,
        target,
        weight,
        metadata,
    })
}

/// Find the first matching field from a list of candidates.
fn find_field(obj: &serde_json::Map<String, Value>, candidates: &[&str]) -> Option<String> {
    for field in candidates {
        if let Some(value) = obj.get(*field) {
            if let Some(s) = value.as_str() {
                return Some(s.to_string());
            }
        }
    }
    None
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
    fn test_parse_instruction_output() {
        let file = write_temp_jsonl(&[r#"{"instruction": "Hello", "output": "World"}"#]);
        let samples = parse_jsonl_file(file.path()).unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].input, "Hello");
        assert_eq!(samples[0].target, "World");
    }

    #[test]
    fn test_parse_prompt_response() {
        let file = write_temp_jsonl(&[r#"{"prompt": "Question?", "response": "Answer."}"#]);
        let samples = parse_jsonl_file(file.path()).unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].input, "Question?");
        assert_eq!(samples[0].target, "Answer.");
    }

    #[test]
    fn test_parse_with_weight() {
        let file = write_temp_jsonl(&[r#"{"input": "a", "output": "b", "weight": 0.5}"#]);
        let samples = parse_jsonl_file(file.path()).unwrap();
        assert!((samples[0].weight - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_skip_empty_lines() {
        let file = write_temp_jsonl(&[
            r#"{"input": "a", "output": "b"}"#,
            "",
            r#"{"input": "c", "output": "d"}"#,
        ]);
        let samples = parse_jsonl_file(file.path()).unwrap();
        assert_eq!(samples.len(), 2);
    }

    #[test]
    fn test_missing_input_field() {
        let file = write_temp_jsonl(&[r#"{"output": "b"}"#]);
        let result = parse_jsonl_file(file.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Missing input field"));
    }

    #[test]
    fn test_empty_input_rejected() {
        let file = write_temp_jsonl(&[r#"{"input": "   ", "output": "b"}"#]);
        let result = parse_jsonl_file(file.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Empty or whitespace-only"));
    }

    #[test]
    fn test_invalid_json() {
        let file = write_temp_jsonl(&[r#"{"input": "a", "output": "b"}"#, r#"not valid json"#]);
        let result = parse_jsonl_file(file.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid JSON"));
    }

    #[test]
    fn test_negative_weight_rejected() {
        let file = write_temp_jsonl(&[r#"{"input": "a", "output": "b", "weight": -0.5}"#]);
        let result = parse_jsonl_file(file.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Negative weight"));
    }
}
