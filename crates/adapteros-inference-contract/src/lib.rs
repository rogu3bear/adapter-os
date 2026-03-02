//! Shared UDS inference transport contract.
//!
//! This crate centralizes stable path constants and stream payload shapes shared
//! by the control plane, API glue, and worker transport layers.

use serde::{Deserialize, Serialize};

/// Canonical external HTTP API prefix.
pub const CANONICAL_HTTP_API_PREFIX: &str = "/v1";
/// Legacy external HTTP API prefix accepted temporarily for compatibility.
pub const LEGACY_HTTP_API_PREFIX: &str = "/api/v1";

/// Canonical UDS inference route.
pub const UDS_INFER_PATH: &str = "/inference";
/// Legacy UDS inference route accepted during migration.
pub const LEGACY_UDS_INFER_PATH: &str = "/api/v1/infer";

/// UDS inference cancellation route prefix.
pub const UDS_INFER_CANCEL_PREFIX: &str = "/inference/cancel";
/// UDS inference resume route prefix.
pub const UDS_INFER_RESUME_PREFIX: &str = "/inference/resume";

/// Legacy `/api/v1/*` deprecation start timestamp (RFC3339).
pub const LEGACY_API_DEPRECATED_AT: &str = "2026-02-21T00:00:00Z";
/// Legacy `/api/v1/*` sunset target timestamp (RFC3339).
pub const LEGACY_API_SUNSET_AT: &str = "2026-08-31T00:00:00Z";

/// Shared canonical worker inference request type.
pub type WorkerInferRequest = adapteros_transport_types::WorkerInferenceRequest;
/// Shared canonical worker request type.
pub type WorkerRequestType = adapteros_transport_types::WorkerRequestType;
/// Shared canonical worker patch request type.
pub type WorkerPatchProposalRequest = adapteros_transport_types::WorkerPatchProposalRequest;

/// Token payload for streaming inference over UDS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStreamTokenPayload {
    pub text: String,
    #[serde(default)]
    pub token_id: Option<u32>,
}

/// Paused event payload for human-in-the-loop review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStreamPausedPayload {
    pub pause_id: String,
    pub inference_id: String,
    pub trigger_kind: String,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub text_so_far: Option<String>,
    pub token_count: usize,
}

/// Rewrite `/api/v1` path to canonical `/v1`.
///
/// Returns `None` when the path is already canonical or does not match the
/// legacy API prefix.
pub fn canonicalize_http_api_path(path: &str) -> Option<String> {
    if path == LEGACY_HTTP_API_PREFIX {
        return Some(CANONICAL_HTTP_API_PREFIX.to_string());
    }

    path.strip_prefix(&(LEGACY_HTTP_API_PREFIX.to_string() + "/"))
        .map(|suffix| format!("{CANONICAL_HTTP_API_PREFIX}/{suffix}"))
}

/// Returns true when the route participates in worker inference auth semantics.
pub fn is_uds_inference_path(path: &str) -> bool {
    matches!(
        path,
        UDS_INFER_PATH | LEGACY_UDS_INFER_PATH | UDS_INFER_CANCEL_PREFIX
    ) || path.starts_with(&(UDS_INFER_CANCEL_PREFIX.to_string() + "/"))
}

/// Legacy API deprecation header value.
pub fn legacy_api_deprecation_header() -> String {
    format!(
        "deprecated_at=\"{}\"; sunset_at=\"{}\"; replacement=\"{}\"",
        LEGACY_API_DEPRECATED_AT, LEGACY_API_SUNSET_AT, CANONICAL_HTTP_API_PREFIX
    )
}

#[cfg(test)]
mod tests {
    use super::{
        canonicalize_http_api_path, is_uds_inference_path, CANONICAL_HTTP_API_PREFIX,
        LEGACY_HTTP_API_PREFIX,
    };

    #[test]
    fn canonicalizes_legacy_http_prefix() {
        assert_eq!(
            canonicalize_http_api_path("/api/v1/system/status").as_deref(),
            Some("/v1/system/status")
        );
        assert_eq!(
            canonicalize_http_api_path(LEGACY_HTTP_API_PREFIX).as_deref(),
            Some(CANONICAL_HTTP_API_PREFIX)
        );
    }

    #[test]
    fn does_not_rewrite_non_legacy_paths() {
        assert!(canonicalize_http_api_path("/v1/system/status").is_none());
        assert!(canonicalize_http_api_path("/healthz").is_none());
    }

    #[test]
    fn detects_inference_routes() {
        assert!(is_uds_inference_path("/inference"));
        assert!(is_uds_inference_path("/api/v1/infer"));
        assert!(is_uds_inference_path("/inference/cancel"));
        assert!(is_uds_inference_path("/inference/cancel/req-1"));
        assert!(!is_uds_inference_path("/inference/resume/pause-1"));
    }
}
