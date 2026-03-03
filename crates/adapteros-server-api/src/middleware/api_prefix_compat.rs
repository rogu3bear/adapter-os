//! Compatibility middleware for legacy `/api/v1/*` request paths.
//!
//! Canonical server routes are defined under `/v1/*`. This middleware rewrites
//! legacy `/api/v1/*` requests to `/v1/*` and emits explicit deprecation headers.

use adapteros_inference_contract::{
    canonicalize_http_api_path, legacy_api_deprecation_header, LEGACY_API_SUNSET_AT,
};
use axum::{
    extract::Request,
    http::{HeaderValue, Uri},
    middleware::Next,
    response::Response,
};

const API_COMPAT_HEADER_NAME: &str = "X-AOS-API-Compat";
const API_COMPAT_HEADER_VALUE: &str = "path-rewritten; from=\"/api/v1\"; to=\"/v1\"";

/// Rewrites `/api/v1/*` to `/v1/*` and annotates responses with deprecation headers.
pub async fn api_prefix_compat_middleware(mut req: Request, next: Next) -> Response {
    let original_path = req.uri().path().to_string();

    let Some(rewritten_path) = canonicalize_http_api_path(&original_path) else {
        return next.run(req).await;
    };

    let rewritten_path_and_query = match req.uri().query() {
        Some(query) => format!("{rewritten_path}?{query}"),
        None => rewritten_path,
    };

    if let Ok(new_uri) = rewritten_path_and_query.parse::<Uri>() {
        *req.uri_mut() = new_uri;
    }

    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    headers.insert(
        API_COMPAT_HEADER_NAME,
        HeaderValue::from_static(API_COMPAT_HEADER_VALUE),
    );

    if !headers.contains_key("X-API-Deprecation") {
        if let Ok(value) = HeaderValue::from_str(&legacy_api_deprecation_header()) {
            headers.insert("X-API-Deprecation", value);
        }
    }

    if !headers.contains_key("Sunset") {
        headers.insert("Sunset", HeaderValue::from_static(LEGACY_API_SUNSET_AT));
    }

    response
}

#[cfg(test)]
mod tests {
    use super::api_prefix_compat_middleware;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        middleware,
        response::Response,
        routing::get,
        Router,
    };
    use tower::{service_fn, Layer, ServiceExt};

    #[tokio::test]
    async fn rewrites_legacy_api_prefix_and_preserves_query() {
        let svc = middleware::from_fn(api_prefix_compat_middleware).layer(service_fn(
            |req: Request<Body>| async move {
                let path_and_query = req
                    .uri()
                    .path_and_query()
                    .map(|pq| pq.as_str().to_string())
                    .unwrap_or_else(|| req.uri().path().to_string());
                Ok::<Response, std::convert::Infallible>(Response::new(Body::from(path_and_query)))
            },
        ));

        let response = svc
            .oneshot(
                Request::builder()
                    .uri("/api/v1/ping?tenant=t-1")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().contains_key("X-AOS-API-Compat"));
        assert!(response.headers().contains_key("X-API-Deprecation"));
        assert!(response.headers().contains_key("Sunset"));

        let body = to_bytes(response.into_body(), 16 * 1024)
            .await
            .expect("body bytes");
        let text = String::from_utf8(body.to_vec()).expect("utf8");
        assert_eq!(text, "/v1/ping?tenant=t-1");
    }

    #[tokio::test]
    async fn leaves_canonical_paths_untouched() {
        let app = Router::new()
            .route("/v1/ping", get(|| async { "ok" }))
            .layer(middleware::from_fn(api_prefix_compat_middleware));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/ping")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert!(!response.headers().contains_key("X-AOS-API-Compat"));
    }
}
