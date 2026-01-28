//! Backend Identity Binding Tests
//!
//! These tests verify that V5+ receipts require backend_id to prevent
//! cross-backend replay attacks. The backend binding ensures that receipts
//! generated on one backend cannot be replayed against another backend.
//!
//! Related to: P0-2 Backend Identity Binding

use adapteros_core::receipt_digest::{
    compute_receipt_digest, compute_v5_digest_checked, validate_backend_id_for_v5,
    ReceiptDigestError, ReceiptDigestInput, RECEIPT_SCHEMA_V4, RECEIPT_SCHEMA_V5,
};

/// Test that validate_backend_id_for_v5 rejects None
#[test]
fn test_validate_backend_id_rejects_none() {
    let result = validate_backend_id_for_v5(None);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ReceiptDigestError::MissingBackendId);
}

/// Test that validate_backend_id_for_v5 rejects empty string
#[test]
fn test_validate_backend_id_rejects_empty() {
    let result = validate_backend_id_for_v5(Some(""));
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ReceiptDigestError::EmptyBackendId);
}

/// Test that validate_backend_id_for_v5 accepts valid backend_id
#[test]
fn test_validate_backend_id_accepts_valid() {
    let result = validate_backend_id_for_v5(Some("mlx"));
    assert!(result.is_ok());

    let result = validate_backend_id_for_v5(Some("coreml"));
    assert!(result.is_ok());

    let result = validate_backend_id_for_v5(Some("metal"));
    assert!(result.is_ok());
}

/// Test that compute_v5_digest_checked rejects empty backend_id
#[test]
fn test_v5_digest_checked_rejects_empty_backend() {
    let input = ReceiptDigestInput::new(
        [0x01u8; 32],
        [0x02u8; 32],
        [0x03u8; 32],
        100,
        10,
        90,
        50,
        50,
    );

    let result = compute_v5_digest_checked(&input, "");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ReceiptDigestError::EmptyBackendId);
}

/// Test that compute_v5_digest_checked accepts valid backend_id
#[test]
fn test_v5_digest_checked_accepts_valid_backend() {
    let input = ReceiptDigestInput::new(
        [0x01u8; 32],
        [0x02u8; 32],
        [0x03u8; 32],
        100,
        10,
        90,
        50,
        50,
    );

    let result = compute_v5_digest_checked(&input, "mlx");
    assert!(result.is_ok());
}

/// Test that different backend_ids produce different V5 digests
#[test]
fn test_different_backends_produce_different_digests() {
    let mut input = ReceiptDigestInput::new(
        [0x01u8; 32],
        [0x02u8; 32],
        [0x03u8; 32],
        100,
        10,
        90,
        50,
        50,
    );

    // Compute digest with "mlx" backend
    input.backend_used = Some("mlx".to_string());
    let digest_mlx = compute_receipt_digest(&input, RECEIPT_SCHEMA_V5).unwrap();

    // Compute digest with "coreml" backend
    input.backend_used = Some("coreml".to_string());
    let digest_coreml = compute_receipt_digest(&input, RECEIPT_SCHEMA_V5).unwrap();

    // Digests should be different
    assert_ne!(
        digest_mlx, digest_coreml,
        "Different backends should produce different V5 digests"
    );
}

/// Test that V5 digest with no backend_used still computes (but should be avoided)
/// This test documents the current behavior - the unchecked compute_receipt_digest
/// allows None backend_used for backward compatibility, but the checked version
/// should be preferred for new code.
#[test]
fn test_v5_unchecked_allows_none_backend() {
    let input = ReceiptDigestInput::new(
        [0x01u8; 32],
        [0x02u8; 32],
        [0x03u8; 32],
        100,
        10,
        90,
        50,
        50,
    );

    // The unchecked version allows None backend_used for backward compatibility
    let result = compute_receipt_digest(&input, RECEIPT_SCHEMA_V5);
    assert!(
        result.is_some(),
        "Unchecked V5 digest should still compute with None backend"
    );
}

/// Test that V4 digests don't include backend_used in hash
/// (V4 is the pre-backend-binding version)
#[test]
fn test_v4_ignores_backend() {
    let mut input = ReceiptDigestInput::new(
        [0x01u8; 32],
        [0x02u8; 32],
        [0x03u8; 32],
        100,
        10,
        90,
        50,
        50,
    );

    // Compute V4 without backend
    let digest_no_backend = compute_receipt_digest(&input, RECEIPT_SCHEMA_V4).unwrap();

    // Compute V4 with backend (should be same since V4 doesn't include it)
    input.backend_used = Some("mlx".to_string());
    let digest_with_backend = compute_receipt_digest(&input, RECEIPT_SCHEMA_V4).unwrap();

    // V4 digests should be the same regardless of backend_used
    assert_eq!(
        digest_no_backend, digest_with_backend,
        "V4 digests should be identical regardless of backend_used"
    );
}

/// Test that ReceiptDigestError display messages are helpful
#[test]
fn test_receipt_digest_error_display() {
    let missing = ReceiptDigestError::MissingBackendId;
    assert!(
        format!("{}", missing).contains("required"),
        "MissingBackendId should mention 'required'"
    );

    let empty = ReceiptDigestError::EmptyBackendId;
    assert!(
        format!("{}", empty).contains("empty"),
        "EmptyBackendId should mention 'empty'"
    );
}

/// Test that the checked digest function produces the same result as unchecked
/// when given a valid backend_id
#[test]
fn test_checked_matches_unchecked_with_valid_backend() {
    let mut input = ReceiptDigestInput::new(
        [0x01u8; 32],
        [0x02u8; 32],
        [0x03u8; 32],
        100,
        10,
        90,
        50,
        50,
    );
    input.backend_used = Some("mlx".to_string());

    // Compute using unchecked (existing function)
    let unchecked_digest = compute_receipt_digest(&input, RECEIPT_SCHEMA_V5).unwrap();

    // Compute using checked function (new function)
    let base_input = ReceiptDigestInput::new(
        [0x01u8; 32],
        [0x02u8; 32],
        [0x03u8; 32],
        100,
        10,
        90,
        50,
        50,
    );
    let checked_digest = compute_v5_digest_checked(&base_input, "mlx").unwrap();

    // Both should produce the same digest
    assert_eq!(
        unchecked_digest, checked_digest,
        "Checked and unchecked functions should produce identical digests"
    );
}

/// Test that backend_id is included in the V5 equipment profile section
#[test]
fn test_v5_equipment_profile_with_backend() {
    let mut input = ReceiptDigestInput::new(
        [0x01u8; 32],
        [0x02u8; 32],
        [0x03u8; 32],
        100,
        10,
        90,
        50,
        50,
    );

    // Set backend and equipment profile
    input.backend_used = Some("mlx".to_string());
    input = input.with_equipment_profile(
        Some([0x07u8; 32]),
        Some("Apple M4 Max".to_string()),
        Some("0.21.0".to_string()),
        Some("ANEv4-38core".to_string()),
    );

    let digest_with_equipment = compute_receipt_digest(&input, RECEIPT_SCHEMA_V5).unwrap();

    // Without equipment profile
    input.equipment_profile_digest_b3 = None;
    input.processor_id = None;
    input.mlx_version = None;
    input.ane_version = None;

    let digest_without_equipment = compute_receipt_digest(&input, RECEIPT_SCHEMA_V5).unwrap();

    // Digests should be different
    assert_ne!(
        digest_with_equipment, digest_without_equipment,
        "Equipment profile should affect V5 digest"
    );
}
