//! Common response wrappers for API handlers
//!
//! This module provides consistent response structures used across
//! all server-api handler crates.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

/// Standard API response wrapper
///
/// Provides a consistent structure for successful API responses.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// Indicates the request was successful
    pub success: bool,
    /// The response data
    pub data: T,
    /// Optional metadata about the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ResponseMeta>,
}

impl<T> ApiResponse<T> {
    /// Create a new successful response with data
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data,
            meta: None,
        }
    }

    /// Create a response with metadata
    pub fn with_meta(data: T, meta: ResponseMeta) -> Self {
        Self {
            success: true,
            data,
            meta: Some(meta),
        }
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        let json = serde_json::to_string(&self).unwrap_or_else(|_| {
            r#"{"success":false,"error":"serialization failed"}"#.to_string()
        });

        (StatusCode::OK, [("content-type", "application/json")], json).into_response()
    }
}

/// Response metadata
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ResponseMeta {
    /// Request ID for tracing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Processing time in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// API version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Empty response for endpoints that return no data
#[derive(Debug, Serialize, Deserialize)]
pub struct EmptyResponse {
    /// Indicates the request was successful
    pub success: bool,
}

impl EmptyResponse {
    /// Create a successful empty response
    pub fn ok() -> Self {
        Self { success: true }
    }
}

impl IntoResponse for EmptyResponse {
    fn into_response(self) -> Response {
        let json = serde_json::to_string(&self).unwrap_or_else(|_| r#"{"success":true}"#.to_string());

        (StatusCode::OK, [("content-type", "application/json")], json).into_response()
    }
}

/// Created response for POST endpoints
#[derive(Debug, Serialize, Deserialize)]
pub struct CreatedResponse<T> {
    /// Indicates the request was successful
    pub success: bool,
    /// The created resource
    pub data: T,
}

impl<T> CreatedResponse<T> {
    /// Create a new created response
    pub fn new(data: T) -> Self {
        Self {
            success: true,
            data,
        }
    }
}

impl<T: Serialize> IntoResponse for CreatedResponse<T> {
    fn into_response(self) -> Response {
        let json = serde_json::to_string(&self).unwrap_or_else(|_| {
            r#"{"success":false,"error":"serialization failed"}"#.to_string()
        });

        (
            StatusCode::CREATED,
            [("content-type", "application/json")],
            json,
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_ok() {
        let response = ApiResponse::ok("test data");
        assert!(response.success);
        assert_eq!(response.data, "test data");
        assert!(response.meta.is_none());
    }

    #[test]
    fn test_api_response_with_meta() {
        let meta = ResponseMeta {
            request_id: Some("req-123".to_string()),
            duration_ms: Some(42),
            version: Some("v1".to_string()),
        };
        let response = ApiResponse::with_meta("test data", meta);
        assert!(response.success);
        assert_eq!(response.data, "test data");
        assert!(response.meta.is_some());

        let meta = response.meta.unwrap();
        assert_eq!(meta.request_id, Some("req-123".to_string()));
        assert_eq!(meta.duration_ms, Some(42));
        assert_eq!(meta.version, Some("v1".to_string()));
    }

    #[test]
    fn test_response_meta_default() {
        let meta = ResponseMeta::default();
        assert!(meta.request_id.is_none());
        assert!(meta.duration_ms.is_none());
        assert!(meta.version.is_none());
    }

    #[test]
    fn test_empty_response() {
        let response = EmptyResponse::ok();
        assert!(response.success);
    }

    #[test]
    fn test_created_response() {
        let response = CreatedResponse::new("new resource");
        assert!(response.success);
        assert_eq!(response.data, "new resource");
    }

    // Serde roundtrip tests
    #[test]
    fn test_api_response_serialize() {
        let response = ApiResponse::ok("test");
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""success":true"#));
        assert!(json.contains(r#""data":"test""#));
        assert!(!json.contains("meta")); // Should skip None
    }

    #[test]
    fn test_api_response_deserialize() {
        let json = r#"{"success":true,"data":"test"}"#;
        let response: ApiResponse<String> = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.data, "test");
        assert!(response.meta.is_none());
    }

    #[test]
    fn test_api_response_with_meta_serialize() {
        let meta = ResponseMeta {
            request_id: Some("req-456".to_string()),
            duration_ms: Some(100),
            version: None,
        };
        let response = ApiResponse::with_meta(42, meta);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""success":true"#));
        assert!(json.contains(r#""data":42"#));
        assert!(json.contains(r#""request_id":"req-456""#));
        assert!(json.contains(r#""duration_ms":100"#));
        assert!(!json.contains("version")); // Should skip None
    }

    #[test]
    fn test_api_response_with_meta_deserialize() {
        let json = r#"{"success":true,"data":99,"meta":{"request_id":"req-789","duration_ms":250}}"#;
        let response: ApiResponse<u32> = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.data, 99);
        assert!(response.meta.is_some());

        let meta = response.meta.unwrap();
        assert_eq!(meta.request_id, Some("req-789".to_string()));
        assert_eq!(meta.duration_ms, Some(250));
    }

    #[test]
    fn test_response_meta_serialize() {
        let meta = ResponseMeta {
            request_id: Some("req-xyz".to_string()),
            duration_ms: None,
            version: Some("v2".to_string()),
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains(r#""request_id":"req-xyz""#));
        assert!(json.contains(r#""version":"v2""#));
        assert!(!json.contains("duration_ms")); // Should skip None
    }

    #[test]
    fn test_response_meta_deserialize() {
        let json = r#"{"request_id":"req-abc","duration_ms":500,"version":"v3"}"#;
        let meta: ResponseMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.request_id, Some("req-abc".to_string()));
        assert_eq!(meta.duration_ms, Some(500));
        assert_eq!(meta.version, Some("v3".to_string()));
    }

    #[test]
    fn test_empty_response_serialize() {
        let response = EmptyResponse::ok();
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"success":true}"#);
    }

    #[test]
    fn test_empty_response_deserialize() {
        let json = r#"{"success":true}"#;
        let response: EmptyResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
    }

    #[test]
    fn test_created_response_serialize() {
        let response = CreatedResponse::new("resource_id_123");
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""success":true"#));
        assert!(json.contains(r#""data":"resource_id_123""#));
    }

    #[test]
    fn test_created_response_deserialize() {
        let json = r#"{"success":true,"data":"resource_id_456"}"#;
        let response: CreatedResponse<String> = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.data, "resource_id_456");
    }

    // IntoResponse tests (verify status codes and content-type)
    #[test]
    fn test_api_response_into_response() {
        let response = ApiResponse::ok("test");
        let axum_response = response.into_response();
        assert_eq!(axum_response.status(), StatusCode::OK);
    }

    #[test]
    fn test_empty_response_into_response() {
        let response = EmptyResponse::ok();
        let axum_response = response.into_response();
        assert_eq!(axum_response.status(), StatusCode::OK);
    }

    #[test]
    fn test_created_response_into_response() {
        let response = CreatedResponse::new("test");
        let axum_response = response.into_response();
        assert_eq!(axum_response.status(), StatusCode::CREATED);
    }

    // Edge cases
    #[test]
    fn test_api_response_with_complex_data() {
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct ComplexData {
            id: u64,
            name: String,
            tags: Vec<String>,
        }

        let data = ComplexData {
            id: 123,
            name: "test".to_string(),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
        };

        let response = ApiResponse::ok(data.clone());
        let json = serde_json::to_string(&response).unwrap();
        let parsed: ApiResponse<ComplexData> = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.data, data);
    }
}
