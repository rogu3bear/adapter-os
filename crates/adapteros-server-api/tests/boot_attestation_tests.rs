//! Integration tests for boot attestation endpoints
//!
//! These tests verify that:
//! 1. POST /v1/system/verify-boot-attestation returns deterministic errors on malformed input
//! 2. Replay protection rejects requests with missing, stale, or future timestamps
//!
//! Note: Auth requirement tests are not included here because the `dev_bypass_status()` uses
//! a static OnceLock which is shared across all tests, making auth testing unreliable.
//! Auth requirements are tested in the consolidated auth tests.

mod common;

use adapteros_server_api::handlers::boot_attestation::ATTESTATION_VERIFY_MAX_AGE_SECS;
use adapteros_server_api::routes;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

/// Helper to get current timestamp in microseconds
fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// Test that verify-boot-attestation returns deterministic error for invalid merkle_root hex
#[tokio::test]
async fn test_verify_attestation_invalid_merkle_root_hex() {
    let _guard = common::TestkitEnvGuard::enabled(true).await; // Enable dev bypass for auth
    let state = common::setup_state(None)
        .await
        .expect("Failed to setup state");
    let app = routes::build(state.clone());

    let request_body = serde_json::json!({
        "merkle_root": "not-valid-hex!!!",
        "signature": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "public_key": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "timestamp_us": now_us()
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/system/verify-boot-attestation")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.expect("Request failed");

    // Should return 400 Bad Request for invalid hex encoding
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Invalid merkle_root hex should return 400"
    );

    // Verify error message is deterministic
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(
        error["message"]
            .as_str()
            .unwrap_or("")
            .contains("Invalid merkle_root hex"),
        "Error should mention invalid merkle_root hex: {:?}",
        error
    );
}

/// Test that verify-boot-attestation returns deterministic error for invalid signature hex
#[tokio::test]
async fn test_verify_attestation_invalid_signature_hex() {
    let _guard = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None)
        .await
        .expect("Failed to setup state");
    let app = routes::build(state.clone());

    let request_body = serde_json::json!({
        "merkle_root": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "signature": "invalid-signature-hex!!!",
        "public_key": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "timestamp_us": now_us()
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/system/verify-boot-attestation")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.expect("Request failed");

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Invalid signature hex should return 400"
    );
}

/// Test that verify-boot-attestation returns deterministic error for invalid public_key hex
#[tokio::test]
async fn test_verify_attestation_invalid_public_key_hex() {
    let _guard = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None)
        .await
        .expect("Failed to setup state");
    let app = routes::build(state.clone());

    let request_body = serde_json::json!({
        "merkle_root": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "signature": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "public_key": "invalid-pubkey!!!",
        "timestamp_us": now_us()
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/system/verify-boot-attestation")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.expect("Request failed");

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Invalid public_key hex should return 400"
    );
}

/// Test that verify-boot-attestation returns structured error for wrong signature length
#[tokio::test]
async fn test_verify_attestation_wrong_signature_length() {
    let _guard = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None)
        .await
        .expect("Failed to setup state");
    let app = routes::build(state.clone());

    // Signature should be 64 bytes (128 hex chars), but we provide only 32 bytes
    let request_body = serde_json::json!({
        "merkle_root": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "signature": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "public_key": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "timestamp_us": now_us()
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/system/verify-boot-attestation")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.expect("Request failed");

    // Should return 200 with valid=false and error message (not a 400, since hex parsing succeeded)
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Wrong signature length should return 200 with valid=false"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(result["valid"], false, "valid should be false");
    assert!(
        result["error"]
            .as_str()
            .unwrap_or("")
            .contains("Invalid signature length"),
        "Error should mention invalid signature length: {:?}",
        result
    );
}

/// Test that verify-boot-attestation returns structured error for wrong public key length
#[tokio::test]
async fn test_verify_attestation_wrong_public_key_length() {
    let _guard = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None)
        .await
        .expect("Failed to setup state");
    let app = routes::build(state.clone());

    // Public key should be 32 bytes (64 hex chars), but we provide 16 bytes
    let request_body = serde_json::json!({
        "merkle_root": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "signature": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "public_key": "00112233445566778899aabbccddeeff",
        "timestamp_us": now_us()
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/system/verify-boot-attestation")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.expect("Request failed");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Wrong public key length should return 200 with valid=false"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(result["valid"], false, "valid should be false");
    assert!(
        result["error"]
            .as_str()
            .unwrap_or("")
            .contains("Invalid public key length"),
        "Error should mention invalid public key length: {:?}",
        result
    );
}

/// Test that verify-boot-attestation rejects invalid signature (signature doesn't match)
#[tokio::test]
async fn test_verify_attestation_signature_mismatch() {
    let _guard = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None)
        .await
        .expect("Failed to setup state");
    let app = routes::build(state.clone());

    // Use syntactically valid but mismatched signature/key/data
    // This should return valid=false with a verification error
    let request_body = serde_json::json!({
        "merkle_root": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "signature": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "public_key": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "timestamp_us": now_us()
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/system/verify-boot-attestation")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.expect("Request failed");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Signature mismatch should return 200 with valid=false"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        result["valid"], false,
        "valid should be false for mismatched signature"
    );
    // Error could be about invalid public key (zero bytes aren't valid ed25519 key) or signature verification
    assert!(
        result["error"].as_str().is_some(),
        "Should have an error message: {:?}",
        result
    );
}

// ====================
// Replay Protection Tests
// ====================

/// Test that verify-boot-attestation rejects requests with missing timestamp
#[tokio::test]
async fn test_verify_attestation_missing_timestamp() {
    let _guard = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None)
        .await
        .expect("Failed to setup state");
    let app = routes::build(state.clone());

    // Request without timestamp_us field
    let request_body = serde_json::json!({
        "merkle_root": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "signature": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "public_key": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/system/verify-boot-attestation")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.expect("Request failed");

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Missing timestamp should return 400"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(
        error["message"]
            .as_str()
            .unwrap_or("")
            .contains("timestamp_us"),
        "Error should mention missing timestamp_us: {:?}",
        error
    );
}

/// Test that verify-boot-attestation rejects requests with stale timestamps (replay protection)
#[tokio::test]
async fn test_verify_attestation_stale_timestamp() {
    let _guard = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None)
        .await
        .expect("Failed to setup state");
    let app = routes::build(state.clone());

    // Timestamp 10 minutes in the past (beyond the 5-minute window)
    let stale_timestamp = now_us() - ((ATTESTATION_VERIFY_MAX_AGE_SECS as u64 + 300) * 1_000_000);

    let request_body = serde_json::json!({
        "merkle_root": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "signature": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "public_key": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "timestamp_us": stale_timestamp
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/system/verify-boot-attestation")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.expect("Request failed");

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Stale timestamp should return 400"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(
        error["message"]
            .as_str()
            .unwrap_or("")
            .contains("timestamp too old"),
        "Error should mention stale timestamp: {:?}",
        error
    );
}

/// Test that verify-boot-attestation rejects requests with future timestamps (clock skew protection)
#[tokio::test]
async fn test_verify_attestation_future_timestamp() {
    let _guard = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None)
        .await
        .expect("Failed to setup state");
    let app = routes::build(state.clone());

    // Timestamp 10 minutes in the future (beyond the 5-minute window)
    let future_timestamp = now_us() + ((ATTESTATION_VERIFY_MAX_AGE_SECS as u64 + 300) * 1_000_000);

    let request_body = serde_json::json!({
        "merkle_root": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "signature": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "public_key": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "timestamp_us": future_timestamp
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/system/verify-boot-attestation")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.expect("Request failed");

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Future timestamp should return 400"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(
        error["message"]
            .as_str()
            .unwrap_or("")
            .contains("timestamp too far in future"),
        "Error should mention future timestamp: {:?}",
        error
    );
}

/// Test that verify-boot-attestation accepts timestamps within the valid window
#[tokio::test]
async fn test_verify_attestation_valid_timestamp() {
    let _guard = common::TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None)
        .await
        .expect("Failed to setup state");
    let app = routes::build(state.clone());

    // Timestamp 2 minutes in the past (within the 5-minute window)
    let valid_timestamp = now_us() - (120 * 1_000_000);

    let request_body = serde_json::json!({
        "merkle_root": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "signature": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "public_key": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        "timestamp_us": valid_timestamp
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/system/verify-boot-attestation")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.expect("Request failed");

    // Should pass timestamp validation but fail on signature (since we're using dummy data)
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Valid timestamp should pass timestamp check and return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // The signature will fail (mismatched data), but the timestamp check passed
    assert_eq!(
        result["valid"], false,
        "Should pass timestamp but fail signature verification"
    );
}
