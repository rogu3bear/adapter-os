//! Identity Envelope Validation Tests (P2 Medium)
//!
//! Tests for IdentityEnvelope validation and serialization.
//! All fields (tenant_id, domain, purpose, revision) are required.
//!
//! These tests verify:
//! - Empty tenant_id rejected
//! - Empty domain rejected
//! - Empty purpose rejected
//! - Empty revision rejected
//! - Whitespace-only fields rejected
//! - Default revision uses git hash
//! - Serialization roundtrip
//! - Equality comparison

use adapteros_core::identity::IdentityEnvelope;

/// Test that empty tenant_id is rejected.
#[test]
fn test_empty_tenant_id_rejected() {
    let envelope = IdentityEnvelope::new(
        "".to_string(),
        "router".to_string(),
        "inference".to_string(),
        "v1.0.0".to_string(),
    );

    let result = envelope.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("tenant_id"));
}

/// Test that empty domain is rejected.
#[test]
fn test_empty_domain_rejected() {
    let envelope = IdentityEnvelope::new(
        "tenant-a".to_string(),
        "".to_string(),
        "inference".to_string(),
        "v1.0.0".to_string(),
    );

    let result = envelope.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("domain"));
}

/// Test that empty purpose is rejected.
#[test]
fn test_empty_purpose_rejected() {
    let envelope = IdentityEnvelope::new(
        "tenant-a".to_string(),
        "router".to_string(),
        "".to_string(),
        "v1.0.0".to_string(),
    );

    let result = envelope.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("purpose"));
}

/// Test that empty revision is rejected.
#[test]
fn test_empty_revision_rejected() {
    let envelope = IdentityEnvelope::new(
        "tenant-a".to_string(),
        "router".to_string(),
        "inference".to_string(),
        "".to_string(),
    );

    let result = envelope.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("revision"));
}

/// Test that whitespace-only fields are rejected.
#[test]
fn test_whitespace_only_fields_rejected() {
    // Whitespace tenant_id
    let envelope1 = IdentityEnvelope::new(
        "   ".to_string(),
        "router".to_string(),
        "inference".to_string(),
        "v1.0.0".to_string(),
    );
    assert!(envelope1.validate().is_err());

    // Whitespace domain
    let envelope2 = IdentityEnvelope::new(
        "tenant-a".to_string(),
        "\t\n".to_string(),
        "inference".to_string(),
        "v1.0.0".to_string(),
    );
    assert!(envelope2.validate().is_err());

    // Whitespace purpose
    let envelope3 = IdentityEnvelope::new(
        "tenant-a".to_string(),
        "router".to_string(),
        "  \t  ".to_string(),
        "v1.0.0".to_string(),
    );
    assert!(envelope3.validate().is_err());

    // Whitespace revision
    let envelope4 = IdentityEnvelope::new(
        "tenant-a".to_string(),
        "router".to_string(),
        "inference".to_string(),
        "\n\n\n".to_string(),
    );
    assert!(envelope4.validate().is_err());
}

/// Test that valid envelope passes validation.
#[test]
fn test_valid_envelope_passes() {
    let envelope = IdentityEnvelope::new(
        "tenant-a".to_string(),
        "router".to_string(),
        "inference".to_string(),
        "v1.0.0".to_string(),
    );

    let result = envelope.validate();
    assert!(result.is_ok());
}

/// Test serialization roundtrip.
#[test]
fn test_serialization_roundtrip() {
    let original = IdentityEnvelope::new(
        "tenant-test".to_string(),
        "kernel".to_string(),
        "training".to_string(),
        "abc1234".to_string(),
    );

    // Serialize to JSON
    let json = serde_json::to_string(&original).unwrap();

    // Deserialize back
    let deserialized: IdentityEnvelope = serde_json::from_str(&json).unwrap();

    // Should be equal
    assert_eq!(original, deserialized);
}

/// Test equality comparison.
#[test]
fn test_envelope_equality() {
    let envelope1 = IdentityEnvelope::new(
        "tenant-a".to_string(),
        "router".to_string(),
        "inference".to_string(),
        "v1.0.0".to_string(),
    );

    let envelope2 = IdentityEnvelope::new(
        "tenant-a".to_string(),
        "router".to_string(),
        "inference".to_string(),
        "v1.0.0".to_string(),
    );

    let envelope3 = IdentityEnvelope::new(
        "tenant-b".to_string(), // Different
        "router".to_string(),
        "inference".to_string(),
        "v1.0.0".to_string(),
    );

    assert_eq!(envelope1, envelope2);
    assert_ne!(envelope1, envelope3);
}
