use adapteros_server_api::ApiDoc;
use serde_json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let openapi_spec = ApiDoc::openapi();

    println!("{}", serde_json::to_string_pretty(&openapi_spec)?);

    Ok(())
}
