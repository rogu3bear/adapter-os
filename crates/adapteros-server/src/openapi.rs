//! OpenAPI specification generation for CI validation

use anyhow::Result;
use std::fs;
use utoipa::OpenApi;

/// Generate OpenAPI specification file from routes
pub fn generate_openapi() -> Result<()> {
    let api_doc = adapteros_server_api::routes::ApiDoc::openapi();
    let json = serde_json::to_string_pretty(&api_doc)?;
    fs::write("openapi.json", json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn openapi_generates_without_error() {
        super::generate_openapi().expect("Failed to generate OpenAPI spec");
    }
}
