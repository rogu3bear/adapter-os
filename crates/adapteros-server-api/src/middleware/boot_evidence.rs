//! Boot evidence middleware
//!
//! Verifies that boot evidence artifacts exist before serving requests.
//! This ensures the appliance produces reliable receipts and maintains
//! traceability for the run environment.

use crate::types::ErrorResponse;
use axum::{
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use std::path::Path;

/// Path to the required boot report file
const BOOT_REPORT_PATH: &str = "var/run/boot_report.json";

/// Middleware that verifies boot evidence exists before serving requests.
///
/// Returns 503 SERVICE_UNAVAILABLE if the boot report file is missing,
/// indicating the system started without proper evidence artifacts.
pub async fn boot_evidence_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let path = req.uri().path();

    // Allow health endpoints to bypass - they need to report status even without evidence
    let bypass = path.starts_with("/healthz")
        || path.starts_with("/readyz")
        || path.starts_with("/system/ready")
        || path.starts_with("/v1/status")
        || path.starts_with("/v1/system/status") // UI dashboard status endpoint
        || path.starts_with("/admin/lifecycle")
        || path.starts_with("/metrics");

    if bypass {
        return Ok(next.run(req).await);
    }

    // Check boot evidence exists
    if !Path::new(BOOT_REPORT_PATH).exists() {
        tracing::error!(
            path = BOOT_REPORT_PATH,
            "Boot evidence missing - refusing to serve requests without audit trail"
        );
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("boot evidence missing")
                    .with_code("BOOT_EVIDENCE_MISSING")
                    .with_string_details(
                        "Server started without boot report - audit trail unavailable",
                    ),
            ),
        ));
    }

    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::get, Router};
    use tower::ServiceExt;

    fn test_app() -> Router {
        Router::new()
            .route("/api/test", get(|| async { StatusCode::OK }))
            .route("/healthz", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn(boot_evidence_middleware))
    }

    #[tokio::test]
    async fn health_endpoint_bypasses_evidence_check() {
        let req = Request::builder()
            .uri("/healthz")
            .body(Body::empty())
            .unwrap();

        let resp = test_app().oneshot(req).await.unwrap();
        // Health endpoint should always work
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
