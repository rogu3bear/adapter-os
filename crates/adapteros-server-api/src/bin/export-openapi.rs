use adapteros_server_api::routes::ApiDoc;
use serde_json::Value;
use std::fs;
use utoipa::OpenApi;

/// Normalize OpenAPI spec to reduce false positive diffs.
///
/// This function applies the following transformations:
/// 1. Removes explicit null fields from objects recursively
/// 2. Preserves empty arrays (they have semantic meaning in OpenAPI, e.g., security scopes)
///
/// # Design Decisions
/// - Null removal: Optional fields serialized as `"field": null` are equivalent
///   to absent fields in JSON/OpenAPI semantics. Removing them reduces spurious diffs
///   when utoipa version or serialization order changes.
/// - Empty array preservation: In OpenAPI, empty arrays like `"bearer_auth": []`
///   indicate "this security scheme is required but with no additional scopes."
///   Removing them would change semantics (making endpoints unsecured).
fn normalize_spec(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // First, recursively normalize all child values
            for v in map.values_mut() {
                normalize_spec(v);
            }

            // Then remove explicit null fields from this object
            // Note: We collect keys to remove first to avoid mutating while iterating
            let null_keys: Vec<String> = map
                .iter()
                .filter(|(_, v)| v.is_null())
                .map(|(k, _)| k.clone())
                .collect();

            for key in null_keys {
                map.remove(&key);
            }
        }
        Value::Array(arr) => {
            // Recursively normalize array elements
            for item in arr.iter_mut() {
                normalize_spec(item);
            }
        }
        _ => {
            // Scalars (string, number, bool, null) need no normalization
        }
    }
}

/// Recursively sort all JSON object keys for deterministic output.
/// This ensures the OpenAPI spec produces identical output across runs,
/// regardless of HashMap iteration order.
fn sort_json_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            // Collect keys, sort them, and rebuild the object
            let mut pairs: Vec<(String, Value)> = map
                .into_iter()
                .map(|(k, v)| (k, sort_json_keys(v)))
                .collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));

            // Use serde_json::Map which preserves insertion order
            let sorted: serde_json::Map<String, Value> = pairs.into_iter().collect();
            Value::Object(sorted)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(sort_json_keys).collect()),
        other => other,
    }
}

fn main() -> anyhow::Result<()> {
    // Generate OpenAPI specification from utoipa annotations
    let openapi = ApiDoc::openapi();

    // Convert to JSON Value first
    let mut value: Value = serde_json::to_value(&openapi)?;

    // Normalize to remove non-semantic differences (e.g., explicit null fields)
    normalize_spec(&mut value);

    // Sort all keys for deterministic output
    let sorted_value = sort_json_keys(value);

    // Serialize to pretty JSON with consistent formatting
    let spec_json = serde_json::to_string_pretty(&sorted_value)?;

    // Get output path from args or use default
    let output_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/codegen/openapi.json".to_string());

    // Ensure output directory exists
    if let Some(parent) = std::path::Path::new(&output_path).parent() {
        fs::create_dir_all(parent)?;
    }

    // Write spec to file
    fs::write(&output_path, spec_json)?;

    println!("✓ OpenAPI spec written to {}", output_path);
    println!("  Paths: {}", openapi.paths.paths.len());
    println!(
        "  Components: {}",
        openapi
            .components
            .as_ref()
            .map(|c| c.schemas.len())
            .unwrap_or(0)
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_normalize_removes_null_fields() {
        let mut input = json!({
            "name": "test",
            "nullable_field": null,
            "active": true
        });

        normalize_spec(&mut input);

        assert_eq!(input.get("name"), Some(&json!("test")));
        assert_eq!(input.get("nullable_field"), None); // Null field removed
        assert_eq!(input.get("active"), Some(&json!(true)));
    }

    #[test]
    fn test_normalize_preserves_empty_arrays() {
        // Empty arrays in OpenAPI have semantic meaning (e.g., security scopes)
        let mut input = json!({
            "security": [
                { "bearer_auth": [] }
            ],
            "tags": []
        });

        normalize_spec(&mut input);

        // Empty arrays should be preserved
        assert_eq!(input.get("security"), Some(&json!([{ "bearer_auth": [] }])));
        assert_eq!(input.get("tags"), Some(&json!([])));
    }

    #[test]
    fn test_normalize_recursive_null_removal() {
        let mut input = json!({
            "outer": {
                "inner": {
                    "value": "keep",
                    "null_field": null
                },
                "another_null": null
            },
            "top_null": null
        });

        normalize_spec(&mut input);

        // All nulls should be removed at every level
        let outer = input.get("outer").unwrap();
        let inner = outer.get("inner").unwrap();
        assert_eq!(inner.get("value"), Some(&json!("keep")));
        assert_eq!(inner.get("null_field"), None);
        assert_eq!(outer.get("another_null"), None);
        assert_eq!(input.get("top_null"), None);
    }

    #[test]
    fn test_normalize_handles_arrays_with_null_containing_objects() {
        let mut input = json!({
            "items": [
                { "id": 1, "optional": null },
                { "id": 2, "optional": "present" }
            ]
        });

        normalize_spec(&mut input);

        let items = input.get("items").unwrap().as_array().unwrap();
        assert_eq!(items[0].get("id"), Some(&json!(1)));
        assert_eq!(items[0].get("optional"), None); // Null removed from array element
        assert_eq!(items[1].get("id"), Some(&json!(2)));
        assert_eq!(items[1].get("optional"), Some(&json!("present")));
    }

    #[test]
    fn test_sort_json_keys_deterministic() {
        // Keys inserted in non-alphabetical order
        let input = json!({
            "zebra": 1,
            "apple": 2,
            "mango": 3
        });

        let sorted = sort_json_keys(input);
        let keys: Vec<&String> = sorted.as_object().unwrap().keys().collect();

        assert_eq!(keys, vec!["apple", "mango", "zebra"]);
    }

    #[test]
    fn test_sort_json_keys_recursive() {
        let input = json!({
            "z_outer": {
                "z_inner": 1,
                "a_inner": 2
            },
            "a_outer": "value"
        });

        let sorted = sort_json_keys(input);

        // Top-level keys sorted
        let top_keys: Vec<&String> = sorted.as_object().unwrap().keys().collect();
        assert_eq!(top_keys, vec!["a_outer", "z_outer"]);

        // Nested keys also sorted
        let nested = sorted.get("z_outer").unwrap().as_object().unwrap();
        let nested_keys: Vec<&String> = nested.keys().collect();
        assert_eq!(nested_keys, vec!["a_inner", "z_inner"]);
    }

    #[test]
    fn test_normalize_and_sort_integration() {
        // Simulate a mini OpenAPI-like structure
        let mut input = json!({
            "paths": {
                "/api/users": {
                    "get": {
                        "summary": "Get users",
                        "deprecated": null,
                        "security": [{ "bearer_auth": [] }]
                    }
                }
            },
            "info": {
                "title": "API",
                "description": null
            }
        });

        // Normalize first
        normalize_spec(&mut input);

        // Then sort
        let result = sort_json_keys(input);

        // Verify nulls removed
        let info = result.get("info").unwrap();
        assert_eq!(info.get("description"), None);
        assert_eq!(info.get("title"), Some(&json!("API")));

        // Verify empty arrays preserved
        let paths = result.get("paths").unwrap();
        let users = paths.get("/api/users").unwrap();
        let get = users.get("get").unwrap();
        assert_eq!(get.get("deprecated"), None);
        assert_eq!(get.get("security"), Some(&json!([{ "bearer_auth": [] }])));

        // Verify key ordering
        let top_keys: Vec<&String> = result.as_object().unwrap().keys().collect();
        assert_eq!(top_keys, vec!["info", "paths"]);
    }
}
