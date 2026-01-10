//! CSV format parser for tabular datasets.
//!
//! Supports configurable column mapping for input and target fields.

use super::{ColumnMapping, RawSample};
use crate::training::normalize::{normalize_text, validate_non_empty};
use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

/// Parse a CSV file into raw samples.
pub fn parse_csv_file(path: &Path, mapping: &ColumnMapping) -> Result<Vec<RawSample>> {
    let file = File::open(path)
        .map_err(|e| AosError::Io(format!("Failed to open CSV file {}: {}", path.display(), e)))?;

    let path_str = path.display().to_string();
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(file);

    // Get headers and resolve column indices
    let headers = reader.headers().map_err(|e| {
        AosError::Validation(format!(
            "Failed to read CSV headers from {}: {}",
            path_str, e
        ))
    })?;

    let input_idx = resolve_column_index(headers, &mapping.input_col, "input", &path_str)?;
    let target_idx = resolve_column_index(headers, &mapping.target_col, "target", &path_str)?;
    let weight_idx = mapping
        .weight_col
        .as_ref()
        .map(|col| resolve_column_index(headers, col, "weight", &path_str))
        .transpose()?;

    // Collect all headers for metadata
    let header_names: Vec<String> = headers.iter().map(|h| h.to_string()).collect();

    let mut samples = Vec::new();

    for (row_idx, record_result) in reader.records().enumerate() {
        let row_num = row_idx + 2; // +1 for 0-index, +1 for header row
        let context = format!("{}:{}", path_str, row_num);

        let record = record_result.map_err(|e| {
            AosError::Validation(format!("Failed to parse CSV row at {}: {}", context, e))
        })?;

        // Extract input
        let input_raw = record.get(input_idx).ok_or_else(|| {
            AosError::Validation(format!(
                "Missing input column '{}' at {}",
                mapping.input_col, context
            ))
        })?;

        // Extract target
        let target_raw = record.get(target_idx).ok_or_else(|| {
            AosError::Validation(format!(
                "Missing target column '{}' at {}",
                mapping.target_col, context
            ))
        })?;

        // Normalize text
        let input = normalize_text(input_raw)?;
        let target = normalize_text(target_raw)?;

        // Validate non-empty
        validate_non_empty(&input, "input", &context)?;
        validate_non_empty(&target, "target", &context)?;

        // Extract weight if present
        let weight = if let Some(idx) = weight_idx {
            record
                .get(idx)
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(1.0)
        } else {
            1.0
        };

        // Build metadata
        let mut metadata = HashMap::new();
        metadata.insert("source_file".to_string(), path_str.clone());
        metadata.insert("source_row".to_string(), row_num.to_string());

        // Add other columns as metadata
        for (i, value) in record.iter().enumerate() {
            if i != input_idx && i != target_idx && weight_idx != Some(i) {
                if let Some(header) = header_names.get(i) {
                    if !value.is_empty() {
                        metadata.insert(header.clone(), value.to_string());
                    }
                }
            }
        }

        samples.push(RawSample {
            input,
            target,
            weight,
            metadata,
        });
    }

    if samples.is_empty() {
        return Err(AosError::Validation(format!(
            "CSV file {} contains no valid samples",
            path_str
        )));
    }

    Ok(samples)
}

/// Resolve a column name or index to a numeric index.
fn resolve_column_index(
    headers: &csv::StringRecord,
    col: &str,
    field_name: &str,
    path: &str,
) -> Result<usize> {
    // Try as numeric index first
    if let Ok(idx) = col.parse::<usize>() {
        if idx < headers.len() {
            return Ok(idx);
        }
        return Err(AosError::Validation(format!(
            "Column index {} for {} is out of range (max: {}) in {}",
            idx,
            field_name,
            headers.len() - 1,
            path
        )));
    }

    // Try as column name
    for (i, header) in headers.iter().enumerate() {
        if header.eq_ignore_ascii_case(col) {
            return Ok(i);
        }
    }

    Err(AosError::Validation(format!(
        "Column '{}' for {} not found in {}. Available columns: {:?}",
        col,
        field_name,
        path,
        headers.iter().collect::<Vec<_>>()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_csv(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", content).unwrap();
        file
    }

    #[test]
    fn test_parse_basic_csv() {
        let file = write_temp_csv("input,target\nHello,World\nFoo,Bar\n");
        let mapping = ColumnMapping::default();
        let samples = parse_csv_file(file.path(), &mapping).unwrap();
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].input, "Hello");
        assert_eq!(samples[0].target, "World");
    }

    #[test]
    fn test_custom_column_names() {
        let file = write_temp_csv("question,answer\nWhat?,This.\n");
        let mapping = ColumnMapping {
            input_col: "question".to_string(),
            target_col: "answer".to_string(),
            weight_col: None,
        };
        let samples = parse_csv_file(file.path(), &mapping).unwrap();
        assert_eq!(samples[0].input, "What?");
        assert_eq!(samples[0].target, "This.");
    }

    #[test]
    fn test_column_by_index() {
        let file = write_temp_csv("a,b,c\n1,2,3\n");
        let mapping = ColumnMapping {
            input_col: "0".to_string(),
            target_col: "2".to_string(),
            weight_col: None,
        };
        let samples = parse_csv_file(file.path(), &mapping).unwrap();
        assert_eq!(samples[0].input, "1");
        assert_eq!(samples[0].target, "3");
    }

    #[test]
    fn test_with_weight_column() {
        let file = write_temp_csv("input,target,weight\na,b,0.5\n");
        let mapping = ColumnMapping {
            input_col: "input".to_string(),
            target_col: "target".to_string(),
            weight_col: Some("weight".to_string()),
        };
        let samples = parse_csv_file(file.path(), &mapping).unwrap();
        assert!((samples[0].weight - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_extra_columns_as_metadata() {
        let file = write_temp_csv("input,target,category,id\na,b,test,123\n");
        let mapping = ColumnMapping::default();
        let samples = parse_csv_file(file.path(), &mapping).unwrap();
        assert_eq!(
            samples[0].metadata.get("category"),
            Some(&"test".to_string())
        );
        assert_eq!(samples[0].metadata.get("id"), Some(&"123".to_string()));
    }

    #[test]
    fn test_missing_column() {
        let file = write_temp_csv("foo,bar\na,b\n");
        let mapping = ColumnMapping::default();
        let result = parse_csv_file(file.path(), &mapping);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_empty_input_rejected() {
        let file = write_temp_csv("input,target\n   ,b\n");
        let mapping = ColumnMapping::default();
        let result = parse_csv_file(file.path(), &mapping);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Empty or whitespace-only"));
    }

    #[test]
    fn test_case_insensitive_headers() {
        let file = write_temp_csv("INPUT,TARGET\na,b\n");
        let mapping = ColumnMapping::default();
        let samples = parse_csv_file(file.path(), &mapping).unwrap();
        assert_eq!(samples.len(), 1);
    }
}
