use adapteros_server_api::routes::ApiDoc;
use serde_json::Value;
use std::fs;
use utoipa::OpenApi;

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

    // Convert to JSON Value first, then sort all keys for deterministic output
    let value: Value = serde_json::to_value(&openapi)?;
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
