use adapteros_server_api::routes::ApiDoc;
use std::fs;
use utoipa::OpenApi;

fn main() -> anyhow::Result<()> {
    // Generate OpenAPI specification from utoipa annotations
    let openapi = ApiDoc::openapi();

    // Serialize to pretty JSON
    let spec_json = serde_json::to_string_pretty(&openapi)?;

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
