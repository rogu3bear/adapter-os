//! Request validation helpers for inference execution.
//!
//! This module provides utilities for validating inference requests,
//! including pinned adapter validation and effective set constraints.

use crate::types::InferenceError;

/// Parse pinned_adapter_ids JSON string to Vec<String>.
///
/// Returns None if the input is None or if parsing fails (malformed JSON
/// is treated as "no pinned adapters" rather than an error).
pub fn parse_pinned_adapter_ids(json: Option<&str>) -> Option<Vec<String>> {
    json.and_then(|s| serde_json::from_str(s).ok())
}

/// Ensure pinned adapters (if any) are within the effective adapter set when present.
pub fn validate_pinned_within_effective_set(
    effective_adapter_ids: &Option<Vec<String>>,
    pinned_adapter_ids: &Option<Vec<String>>,
) -> Result<(), InferenceError> {
    if let (Some(effective), Some(pinned)) = (effective_adapter_ids, pinned_adapter_ids) {
        if effective.is_empty() {
            return Ok(());
        }
        for pinned_id in pinned {
            if !effective.iter().any(|id| id == pinned_id) {
                return Err(InferenceError::ValidationError(format!(
                    "Pinned adapter '{}' is not in effective_adapter_ids: {:?}",
                    pinned_id, effective
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pinned_adapter_ids_valid_json() {
        let result = parse_pinned_adapter_ids(Some(r#"["adapter-a", "adapter-b"]"#));
        assert_eq!(
            result,
            Some(vec!["adapter-a".to_string(), "adapter-b".to_string()])
        );
    }

    #[test]
    fn test_parse_pinned_adapter_ids_empty_array() {
        let result = parse_pinned_adapter_ids(Some("[]"));
        assert_eq!(result, Some(vec![]));
    }

    #[test]
    fn test_parse_pinned_adapter_ids_none_input() {
        let result = parse_pinned_adapter_ids(None);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_pinned_adapter_ids_invalid_json() {
        // Malformed JSON should return None (not panic)
        let result = parse_pinned_adapter_ids(Some("not valid json"));
        assert!(result.is_none());
    }

    #[test]
    fn test_validate_pinned_within_effective_set_success() {
        let effective = Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        let pinned = Some(vec!["a".to_string(), "c".to_string()]);

        let result = validate_pinned_within_effective_set(&effective, &pinned);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_pinned_within_effective_set_pinned_outside_fails() {
        let effective = Some(vec!["a".to_string(), "b".to_string()]);
        let pinned = Some(vec!["a".to_string(), "not-in-effective".to_string()]);

        let result = validate_pinned_within_effective_set(&effective, &pinned);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not-in-effective"));
    }

    #[test]
    fn test_validate_pinned_within_effective_set_empty_effective_passes() {
        // Empty effective set allows any pinned (no restriction enforced)
        let effective = Some(vec![]);
        let pinned = Some(vec!["any".to_string()]);

        let result = validate_pinned_within_effective_set(&effective, &pinned);
        assert!(result.is_ok());
    }
}
