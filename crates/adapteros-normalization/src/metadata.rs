//! Metadata extraction and sanitization utilities.

use crate::repo_id::normalize_repo_id;
use serde_json::Value;

/// Extract a repository identifier from JSON metadata.
///
/// Searches for repository identifier in metadata JSON, trying these keys in order:
/// - `repo_identifier`
/// - `scope_repo_id`
/// - `repo_id`
///
/// The extracted value is normalized using [`sanitize_repo_identifier`].
///
/// # Examples
///
/// ```
/// use adapteros_normalization::extract_repo_identifier_from_metadata;
///
/// let metadata = r#"{"repo_id": "https://github.com/org/repo"}"#;
/// assert_eq!(
///     extract_repo_identifier_from_metadata(Some(metadata)),
///     Some("github.com/org/repo".to_string())
/// );
///
/// assert_eq!(extract_repo_identifier_from_metadata(None), None);
/// assert_eq!(extract_repo_identifier_from_metadata(Some("")), None);
/// ```
pub fn extract_repo_identifier_from_metadata(metadata_json: Option<&str>) -> Option<String> {
    let raw = metadata_json?;
    if raw.trim().is_empty() {
        return None;
    }

    let parsed: Value = serde_json::from_str(raw).ok()?;
    for key in ["repo_identifier", "scope_repo_id", "repo_id"] {
        if let Some(value) = parsed.get(key).and_then(|v| v.as_str()) {
            if let Some(repo_id) = sanitize_repo_identifier(Some(value)) {
                return Some(repo_id);
            }
        }
    }

    None
}

/// Sanitize and normalize a repository identifier.
///
/// Trims whitespace, filters empty strings, and applies [`normalize_repo_id`].
///
/// # Examples
///
/// ```
/// use adapteros_normalization::sanitize_repo_identifier;
///
/// assert_eq!(
///     sanitize_repo_identifier(Some("  https://github.com/org/repo  ")),
///     Some("github.com/org/repo".to_string())
/// );
/// assert_eq!(sanitize_repo_identifier(Some("")), None);
/// assert_eq!(sanitize_repo_identifier(None), None);
/// ```
pub fn sanitize_repo_identifier(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(normalize_repo_id)
}

/// Sanitize a repository slug value.
///
/// Trims whitespace and filters empty strings. Does NOT apply slug normalization -
/// that should be done separately with [`crate::normalize_repo_slug`] if needed.
///
/// # Examples
///
/// ```
/// use adapteros_normalization::sanitize_repo_slug;
///
/// assert_eq!(sanitize_repo_slug(Some("  my-repo  ")), Some("my-repo".to_string()));
/// assert_eq!(sanitize_repo_slug(Some("")), None);
/// assert_eq!(sanitize_repo_slug(None), None);
/// ```
pub fn sanitize_repo_slug(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Sanitize an optional string value.
///
/// Trims whitespace and returns None for empty strings.
///
/// # Examples
///
/// ```
/// use adapteros_normalization::sanitize_optional;
///
/// assert_eq!(sanitize_optional(Some("  value  ")), Some("value".to_string()));
/// assert_eq!(sanitize_optional(Some("")), None);
/// assert_eq!(sanitize_optional(None), None);
/// ```
pub fn sanitize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_repo_identifier_finds_repo_id() {
        let metadata = r#"{"repo_id": "https://github.com/org/repo"}"#;
        assert_eq!(
            extract_repo_identifier_from_metadata(Some(metadata)),
            Some("github.com/org/repo".to_string())
        );
    }

    #[test]
    fn extract_repo_identifier_finds_repo_identifier() {
        let metadata = r#"{"repo_identifier": "github.com/org/repo"}"#;
        assert_eq!(
            extract_repo_identifier_from_metadata(Some(metadata)),
            Some("github.com/org/repo".to_string())
        );
    }

    #[test]
    fn extract_repo_identifier_finds_scope_repo_id() {
        let metadata = r#"{"scope_repo_id": "github.com/org/repo"}"#;
        assert_eq!(
            extract_repo_identifier_from_metadata(Some(metadata)),
            Some("github.com/org/repo".to_string())
        );
    }

    #[test]
    fn extract_repo_identifier_prefers_repo_identifier() {
        let metadata =
            r#"{"repo_identifier": "first", "scope_repo_id": "second", "repo_id": "third"}"#;
        assert_eq!(
            extract_repo_identifier_from_metadata(Some(metadata)),
            Some("first".to_string())
        );
    }

    #[test]
    fn extract_repo_identifier_handles_none() {
        assert_eq!(extract_repo_identifier_from_metadata(None), None);
    }

    #[test]
    fn extract_repo_identifier_handles_empty() {
        assert_eq!(extract_repo_identifier_from_metadata(Some("")), None);
        assert_eq!(extract_repo_identifier_from_metadata(Some("   ")), None);
    }

    #[test]
    fn extract_repo_identifier_handles_invalid_json() {
        assert_eq!(
            extract_repo_identifier_from_metadata(Some("not json")),
            None
        );
    }

    #[test]
    fn sanitize_repo_identifier_normalizes() {
        assert_eq!(
            sanitize_repo_identifier(Some("  https://github.com/org/repo  ")),
            Some("github.com/org/repo".to_string())
        );
    }

    #[test]
    fn sanitize_repo_identifier_handles_empty() {
        assert_eq!(sanitize_repo_identifier(Some("")), None);
        assert_eq!(sanitize_repo_identifier(Some("   ")), None);
        assert_eq!(sanitize_repo_identifier(None), None);
    }

    #[test]
    fn sanitize_repo_slug_trims() {
        assert_eq!(
            sanitize_repo_slug(Some("  my-repo  ")),
            Some("my-repo".to_string())
        );
    }

    #[test]
    fn sanitize_repo_slug_handles_empty() {
        assert_eq!(sanitize_repo_slug(Some("")), None);
        assert_eq!(sanitize_repo_slug(None), None);
    }

    #[test]
    fn sanitize_optional_works() {
        assert_eq!(
            sanitize_optional(Some("  value  ")),
            Some("value".to_string())
        );
        assert_eq!(sanitize_optional(Some("")), None);
        assert_eq!(sanitize_optional(None), None);
    }
}
