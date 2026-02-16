use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use rust_embed::RustEmbed;
use std::path::{Component, Path, PathBuf};
use tokio::fs;

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
        // SPA fallback should not catch /api paths
        .fallback(spa_fallback)
}

async fn spa_fallback(uri: Uri) -> impl IntoResponse {
    // Don't serve SPA for API routes - let them 404 properly
    if uri.path().starts_with("/api/") || uri.path().starts_with("/v1/") {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    }

    // Check if this is a static asset (CSS, JS, WASM, etc.) before SPA fallback
    let path = uri.path().trim_start_matches('/');
    if let Some(response) = serve_asset_response(path).await {
        return response.into_response();
    }

    index_handler().await.into_response()
}

async fn index_handler() -> impl IntoResponse {
    match <Assets as RustEmbed>::get("index.html") {
        Some(content) => Html(content.data.into_owned()).into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "UI not built. Run: cd crates/adapteros-ui && trunk build --release",
        )
            .into_response(),
    }
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
