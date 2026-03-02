use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use rust_embed::RustEmbed;
use std::path::{Component, Path, PathBuf};
use tokio::fs;
use tracing::warn;

#[path = "ssr.rs"]
mod ssr;

const SSR_HEADER_NAME: &str = "X-AOS-SSR";
const HTML_CACHE_CONTROL: &str = "no-cache, must-revalidate";

// Note: This will be empty until web-ui is built with trunk
#[derive(RustEmbed)]
#[folder = "static/"]
#[prefix = ""]
struct Assets;

pub fn routes() -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/index.html", get(index_handler))
        // Only serve static assets from known paths, not /api/*
        .route("/static/{*file}", get(static_handler))
        .route("/assets/{*file}", get(static_handler))
        .route("/favicon.ico", get(static_handler))
        .route("/favicon.svg", get(static_handler))
        // SPA fallback should not catch /api paths
        .fallback(spa_fallback)
}

async fn spa_fallback(uri: Uri) -> impl IntoResponse {
    // Don't serve SPA for API routes - let them 404 properly
    if is_api_path(uri.path()) {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    }

    // Check if this is a static asset (CSS, JS, WASM, etc.) before SPA fallback
    let path = uri.path().trim_start_matches('/');
    if let Some(response) = serve_asset_response(path).await {
        return response.into_response();
    }

    // Missing static assets (e.g. .js, .wasm, .css) must return 404, not SPA HTML.
    // Otherwise the browser parses HTML as script and reports "unsupported MIME type".
    if is_static_asset_extension(path) {
        return not_found_asset(path).await.into_response();
    }

    render_index_for_uri(&uri).await
}

async fn index_handler(uri: Uri) -> impl IntoResponse {
    render_index_for_uri(&uri).await
}

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    match serve_asset_response(path).await {
        Some(response) => response.into_response(),
        None => not_found().await.into_response(),
    }
}

async fn serve_asset_response(path: &str) -> Option<Response<Body>> {
    if let Some(content) = Assets::get(path) {
        return build_asset_response(content.data.into_owned(), path);
    }

    if let Some(body) = read_disk_asset(path).await {
        return build_asset_response(body, path);
    }

    None
}

fn build_asset_response(body: Vec<u8>, path: &str) -> Option<Response<Body>> {
    let mime = mime_guess::from_path(path).first_or_octet_stream();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(Body::from(body))
        .ok()
}

async fn read_disk_asset(path: &str) -> Option<Vec<u8>> {
    let asset_path = static_asset_path(path)?;
    fs::read(asset_path).await.ok()
}

fn static_asset_path(path: &str) -> Option<PathBuf> {
    if path.is_empty() {
        return None;
    }

    let relative = Path::new(path);
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }

    Some(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("static")
            .join(relative),
    )
}

async fn not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "404 Not Found")
}

async fn render_index_for_uri(uri: &Uri) -> Response {
    let Some(template) = load_index_template() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "UI not built. Run: cd crates/adapteros-ui && trunk build --release",
        )
            .into_response();
    };

    let path_and_query = path_and_query_for_render(uri);
    render_index_from_template(&template, &path_and_query, try_render_ssr_document)
}

fn load_index_template() -> Option<String> {
    let content = <Assets as RustEmbed>::get("index.html")?;
    String::from_utf8(content.data.into_owned()).ok()
}

fn path_and_query_for_render(uri: &Uri) -> String {
    let path_and_query = uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(uri.path());

    if path_and_query == "/index.html" {
        "/".to_string()
    } else if path_and_query.starts_with("/index.html?") {
        path_and_query.replacen("/index.html", "/", 1)
    } else if path_and_query.is_empty() {
        "/".to_string()
    } else {
        path_and_query.to_string()
    }
}

fn try_render_ssr_document(template: &str, path_and_query: &str) -> Result<String, String> {
    ssr::render_document(template, path_and_query).map_err(|err| err.to_string())
}

fn render_index_from_template(
    template: &str,
    path_and_query: &str,
    renderer: fn(&str, &str) -> Result<String, String>,
) -> Response {
    match renderer(template, path_and_query) {
        Ok(rendered) => html_response(StatusCode::OK, rendered, true),
        Err(err) => {
            warn!(
                path = %path_and_query,
                error = %err,
                "SSR render failed; falling back to static UI shell"
            );
            html_response(StatusCode::OK, template.to_string(), false)
        }
    }
}

fn html_response(status: StatusCode, html: String, ssr_enabled: bool) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, HTML_CACHE_CONTROL)
        .header(SSR_HEADER_NAME, if ssr_enabled { "1" } else { "0" })
        .body(Body::from(html))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Internal Server Error"))
                .unwrap_or_default()
        })
}

fn is_api_path(path: &str) -> bool {
    matches!(path, "/api" | "/v1") || path.starts_with("/api/") || path.starts_with("/v1/")
}

fn is_static_asset_extension(path: &str) -> bool {
    path.ends_with(".js")
        || path.ends_with(".wasm")
        || path.ends_with(".css")
        || path.ends_with(".woff2")
        || path.ends_with(".map")
        || path.ends_with(".svg")
        || path.ends_with(".png")
        || path.ends_with(".jpg")
        || path.ends_with(".jpeg")
        || path.ends_with(".webp")
        || path.ends_with(".ico")
        || path.ends_with(".txt")
        || path.ends_with(".json")
        || path.ends_with(".webmanifest")
}

/// Developer-friendly 404 for missing static assets.
/// Suggests running the UI build when .js/.wasm/.css/.woff2 are missing.
async fn not_found_asset(path: &str) -> impl IntoResponse {
    let body = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>Asset Not Found</title></head>
<body style="font-family:system-ui,sans-serif;max-width:42rem;margin:2rem auto;padding:1rem;">
  <h1>404 — Asset not found</h1>
  <p><code>{path}</code></p>
  <p>Static assets may be missing. Try:</p>
  <pre style="background:#f4f4f4;padding:1rem;border-radius:0.25rem;">./scripts/build-ui.sh</pre>
  <p>Or from the UI crate: <code>cd crates/adapteros-ui && trunk build --release</code></p>
</body>
</html>"#
    );
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(body))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Internal Server Error"))
                .unwrap_or_default()
        })
}

#[cfg(test)]
mod tests {
    use super::{
        is_api_path, is_static_asset_extension, render_index_from_template, SSR_HEADER_NAME,
    };
    use axum::body::to_bytes;
    use axum::http::StatusCode;

    #[test]
    fn api_path_bypass_classification() {
        assert!(is_api_path("/api"));
        assert!(is_api_path("/v1"));
        assert!(is_api_path("/api/ping"));
        assert!(is_api_path("/v1/system/info"));
        assert!(!is_api_path("/dashboard"));
    }

    #[test]
    fn static_asset_extension_classification() {
        assert!(is_static_asset_extension("assets/main.js"));
        assert!(is_static_asset_extension("assets/main.wasm"));
        assert!(is_static_asset_extension("assets/main.css"));
        assert!(is_static_asset_extension("assets/main.woff2"));
        assert!(is_static_asset_extension("assets/main.js.map"));
        assert!(is_static_asset_extension("assets/logo.svg"));
        assert!(is_static_asset_extension("assets/logo.png"));
        assert!(is_static_asset_extension("assets/logo.jpg"));
        assert!(is_static_asset_extension("assets/logo.jpeg"));
        assert!(is_static_asset_extension("assets/logo.webp"));
        assert!(is_static_asset_extension("favicon.ico"));
        assert!(is_static_asset_extension("robots.txt"));
        assert!(is_static_asset_extension("manifest.json"));
        assert!(is_static_asset_extension("site.webmanifest"));
        assert!(!is_static_asset_extension("dashboard"));
    }

    #[tokio::test]
    async fn ssr_injection_success_path_sets_header_and_body(
    ) -> Result<(), Box<dyn std::error::Error>> {
        fn renderer_ok(template: &str, _path: &str) -> Result<String, String> {
            Ok(template.replace("<!--AOS_SSR_OUTLET-->", "<main>SSR OK</main>"))
        }

        let template = "<html><body><!--AOS_SSR_OUTLET--></body></html>";
        let response = render_index_from_template(template, "/dashboard", renderer_ok);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(SSR_HEADER_NAME)
                .and_then(|v| v.to_str().ok()),
            Some("1")
        );
        let body = to_bytes(response.into_body(), 1024 * 1024).await?;
        let html = String::from_utf8(body.to_vec())?;
        assert!(html.contains("SSR OK"));
        Ok(())
    }

    #[tokio::test]
    async fn ssr_failure_falls_back_to_static_template() -> Result<(), Box<dyn std::error::Error>> {
        fn renderer_err(_template: &str, _path: &str) -> Result<String, String> {
            Err("renderer unavailable".to_string())
        }

        let template = "<html><body>STATIC TEMPLATE</body></html>";
        let response = render_index_from_template(template, "/dashboard", renderer_err);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(SSR_HEADER_NAME)
                .and_then(|v| v.to_str().ok()),
            Some("0")
        );
        let body = to_bytes(response.into_body(), 1024 * 1024).await?;
        let html = String::from_utf8(body.to_vec())?;
        assert!(html.contains("STATIC TEMPLATE"));
        Ok(())
    }
}
