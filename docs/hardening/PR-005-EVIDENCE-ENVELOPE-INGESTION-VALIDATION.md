# PR-005: Evidence Envelope Server Ingestion Validation

## Summary

Validate evidence envelopes at submission time (before storage) to reject tampered or malformed envelopes, preventing corrupted data from entering the audit chain.

## Problem Statement

Currently, evidence envelopes are validated primarily at storage time in `store_evidence_envelope()` which checks chain linkage. However:

1. **Root hash not recomputed**: Envelope claims a `root` but it's not verified against payload
2. **Signature validation deferred**: Signature checking requires feature flag and isn't enforced on ingestion
3. **Payload completeness unchecked**: Missing or malformed payload refs not detected
4. **Invalid envelopes may persist**: Partial validation allows corrupted envelopes into DB

A compromised client or worker could submit envelopes with falsified roots that pass chain linkage but fail forensic verification.

## Solution

1. Add comprehensive pre-storage validation in evidence submission handler
2. Recompute and verify envelope root against payload before storage
3. Enforce signature validation on ingestion (not just export)
4. Reject invalid envelopes with specific error codes
5. Emit `audit_chain_divergence_event()` on validation rejection

---

## Implementation Details

### File Changes

#### 1. `crates/adapteros-core/src/evidence_verifier.rs`

**Add comprehensive ingestion validator**:

```rust
use crate::evidence_envelope::{
    EvidenceEnvelope, EvidenceScope, BundleMetadataRef, PolicyAuditRef, InferenceReceiptRef,
};
use crate::B3Hash;

/// Result of evidence envelope ingestion validation.
#[derive(Debug, Clone)]
pub struct IngestionValidationResult {
    pub is_valid: bool,
    pub errors: Vec<IngestionError>,
    pub warnings: Vec<String>,
    pub computed_root: Option<B3Hash>,
}

/// Specific error types for ingestion validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IngestionError {
    /// Schema version not supported
    UnsupportedSchema { version: u8, max_supported: u8 },

    /// Root hash mismatch (tampered payload)
    RootMismatch { claimed: B3Hash, computed: B3Hash },

    /// Signature validation failed
    SignatureInvalid { reason: String },

    /// Signature missing when required
    SignatureMissing,

    /// Payload reference missing for claimed scope
    PayloadMissing { scope: EvidenceScope },

    /// Payload reference present for wrong scope
    PayloadScopeMismatch { claimed: EvidenceScope, present: EvidenceScope },

    /// Chain linkage invalid
    ChainLinkageInvalid { reason: String },

    /// Previous root mismatch
    PreviousRootMismatch { expected: Option<B3Hash>, claimed: Option<B3Hash> },

    /// Tenant ID mismatch with payload
    TenantMismatch { envelope: String, payload: String },

    /// Timestamp in future
    TimestampInFuture { timestamp: String },

    /// Key ID doesn't match public key
    KeyIdMismatch { computed: String, claimed: String },
}

impl std::fmt::Display for IngestionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchema { version, max_supported } =>
                write!(f, "Schema version {} not supported (max: {})", version, max_supported),
            Self::RootMismatch { claimed, computed } =>
                write!(f, "Root mismatch: claimed {} != computed {}",
                    claimed.to_short_hex(), computed.to_short_hex()),
            Self::SignatureInvalid { reason } =>
                write!(f, "Invalid signature: {}", reason),
            Self::SignatureMissing =>
                write!(f, "Signature required but missing"),
            Self::PayloadMissing { scope } =>
                write!(f, "Payload missing for scope {:?}", scope),
            Self::PayloadScopeMismatch { claimed, present } =>
                write!(f, "Scope mismatch: claimed {:?} but payload is {:?}", claimed, present),
            Self::ChainLinkageInvalid { reason } =>
                write!(f, "Chain linkage invalid: {}", reason),
            Self::PreviousRootMismatch { expected, claimed } =>
                write!(f, "Previous root mismatch: expected {:?}, claimed {:?}", expected, claimed),
            Self::TenantMismatch { envelope, payload } =>
                write!(f, "Tenant mismatch: envelope={} payload={}", envelope, payload),
            Self::TimestampInFuture { timestamp } =>
                write!(f, "Timestamp in future: {}", timestamp),
            Self::KeyIdMismatch { computed, claimed } =>
                write!(f, "Key ID mismatch: computed {} != claimed {}", computed, claimed),
        }
    }
}

/// Validate an evidence envelope for ingestion.
///
/// Performs comprehensive validation including:
/// 1. Schema version check
/// 2. Root hash recomputation and verification
/// 3. Signature validation (if feature enabled)
/// 4. Payload presence and scope matching
/// 5. Timestamp validity check
/// 6. Key ID verification
///
/// Does NOT check chain linkage (requires DB state).
pub fn validate_for_ingestion(
    envelope: &EvidenceEnvelope,
    require_signature: bool,
) -> IngestionValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // 1. Schema version check
    if envelope.schema_version > EVIDENCE_ENVELOPE_SCHEMA_VERSION {
        errors.push(IngestionError::UnsupportedSchema {
            version: envelope.schema_version,
            max_supported: EVIDENCE_ENVELOPE_SCHEMA_VERSION,
        });
    }

    // 2. Verify payload presence matches scope
    let payload_scope = detect_payload_scope(envelope);
    match payload_scope {
        Some(scope) if scope != envelope.scope => {
            errors.push(IngestionError::PayloadScopeMismatch {
                claimed: envelope.scope,
                present: scope,
            });
        }
        None => {
            errors.push(IngestionError::PayloadMissing {
                scope: envelope.scope,
            });
        }
        _ => {}
    }

    // 3. Recompute and verify root hash
    let computed_root = compute_envelope_root(envelope);
    if let Some(ref computed) = computed_root {
        if computed != &envelope.root {
            errors.push(IngestionError::RootMismatch {
                claimed: envelope.root,
                computed: *computed,
            });
        }
    }

    // 4. Signature validation
    if require_signature {
        if envelope.signature.is_empty() {
            errors.push(IngestionError::SignatureMissing);
        } else {
            #[cfg(feature = "evidence-signing")]
            {
                match verify_envelope_signature(envelope) {
                    Ok(true) => {}
                    Ok(false) => {
                        errors.push(IngestionError::SignatureInvalid {
                            reason: "Signature verification returned false".to_string(),
                        });
                    }
                    Err(e) => {
                        errors.push(IngestionError::SignatureInvalid {
                            reason: e.to_string(),
                        });
                    }
                }
            }
            #[cfg(not(feature = "evidence-signing"))]
            {
                warnings.push("Signature present but evidence-signing feature not enabled".to_string());
            }
        }
    }

    // 5. Key ID verification (if public key present)
    if !envelope.public_key.is_empty() {
        let computed_key_id = compute_key_id_from_pubkey(&envelope.public_key);
        if computed_key_id != envelope.key_id {
            errors.push(IngestionError::KeyIdMismatch {
                computed: computed_key_id,
                claimed: envelope.key_id.clone(),
            });
        }
    }

    // 6. Timestamp sanity (not too far in future)
    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&envelope.created_at) {
        let now = chrono::Utc::now();
        let max_future = now + chrono::Duration::minutes(5);
        if ts > max_future {
            errors.push(IngestionError::TimestampInFuture {
                timestamp: envelope.created_at.clone(),
            });
        }
    }

    IngestionValidationResult {
        is_valid: errors.is_empty(),
        errors,
        warnings,
        computed_root,
    }
}

/// Detect which payload reference is present in the envelope.
fn detect_payload_scope(envelope: &EvidenceEnvelope) -> Option<EvidenceScope> {
    if envelope.bundle_metadata_ref.is_some() {
        Some(EvidenceScope::Telemetry)
    } else if envelope.policy_audit_ref.is_some() {
        Some(EvidenceScope::Policy)
    } else if envelope.inference_receipt_ref.is_some() {
        Some(EvidenceScope::Inference)
    } else {
        None
    }
}

/// Compute envelope root from payload reference.
fn compute_envelope_root(envelope: &EvidenceEnvelope) -> Option<B3Hash> {
    match envelope.scope {
        EvidenceScope::Telemetry => {
            envelope.bundle_metadata_ref.as_ref().map(|r| {
                B3Hash::hash_multi(&[
                    r.bundle_hash.as_bytes(),
                    r.merkle_root.as_bytes(),
                ])
            })
        }
        EvidenceScope::Policy => {
            envelope.policy_audit_ref.as_ref().map(|r| r.entry_hash)
        }
        EvidenceScope::Inference => {
            envelope.inference_receipt_ref.as_ref().map(|r| r.receipt_digest)
        }
    }
}

/// Compute key ID from public key bytes.
fn compute_key_id_from_pubkey(pubkey_hex: &str) -> String {
    if let Ok(bytes) = hex::decode(pubkey_hex) {
        let hash = B3Hash::hash(&bytes);
        // Key ID is first 16 bytes of BLAKE3 hash, hex encoded
        hex::encode(&hash.as_bytes()[..16])
    } else {
        String::new()
    }
}

#[cfg(feature = "evidence-signing")]
fn verify_envelope_signature(envelope: &EvidenceEnvelope) -> Result<bool, Box<dyn std::error::Error>> {
    use ed25519_dalek::{Signature, VerifyingKey};

    let pubkey_bytes = hex::decode(&envelope.public_key)?;
    let signature_bytes = hex::decode(&envelope.signature)?;

    let verifying_key = VerifyingKey::from_bytes(
        pubkey_bytes.as_slice().try_into()?
    )?;
    let signature = Signature::from_bytes(
        signature_bytes.as_slice().try_into()?
    );

    let canonical_bytes = envelope.to_canonical_bytes();
    Ok(verifying_key.verify_strict(&canonical_bytes, &signature).is_ok())
}
```

#### 2. `crates/adapteros-server-api/src/handlers/evidence.rs`

**Add ingestion validation handler**:

```rust
use adapteros_core::evidence_verifier::{
    validate_for_ingestion, IngestionValidationResult, IngestionError,
};
use adapteros_core::telemetry::audit_chain_divergence_event;

/// Submit an evidence envelope for storage.
///
/// Validates the envelope before storage and rejects invalid submissions.
#[axum::debug_handler]
pub async fn submit_evidence_envelope(
    State(state): State<AppState>,
    Json(envelope): Json<EvidenceEnvelope>,
) -> Result<Json<SubmitEvidenceResponse>, ApiError> {
    // 1. Validate envelope structure
    let require_signature = state.config.evidence.require_signatures;
    let validation = validate_for_ingestion(&envelope, require_signature);

    if !validation.is_valid {
        // Log validation failures
        tracing::warn!(
            tenant_id = %envelope.tenant_id,
            scope = ?envelope.scope,
            errors = ?validation.errors,
            "Evidence envelope validation failed"
        );

        // Emit observability event for audit trail
        let error_summary = validation.errors.iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ");

        let event = audit_chain_divergence_event(
            format!("Evidence envelope rejected: {}", error_summary),
            None,
            Some(envelope.tenant_id.clone()),
            None,
        );
        emit_observability_event(&event);

        // Increment rejection metric
        metrics::counter!(
            "evidence_envelope_rejected_total",
            "tenant_id" => envelope.tenant_id.clone(),
            "scope" => format!("{:?}", envelope.scope),
            "error_type" => categorize_error(&validation.errors),
        ).increment(1);

        // Return detailed error
        return Err(ApiError::BadRequest(EvidenceValidationError {
            errors: validation.errors,
            warnings: validation.warnings,
        }));
    }

    // 2. Validate chain linkage (requires DB lookup)
    let chain_validation = validate_chain_linkage(
        &state.db_pool,
        &envelope,
    ).await?;

    if let Some(error) = chain_validation {
        tracing::warn!(
            tenant_id = %envelope.tenant_id,
            scope = ?envelope.scope,
            error = ?error,
            "Evidence envelope chain linkage invalid"
        );

        let event = audit_chain_divergence_event(
            format!("Chain linkage rejected: {}", error),
            None,
            Some(envelope.tenant_id.clone()),
            None,
        );
        emit_observability_event(&event);

        metrics::counter!(
            "evidence_envelope_rejected_total",
            "tenant_id" => envelope.tenant_id.clone(),
            "scope" => format!("{:?}", envelope.scope),
            "error_type" => "chain_linkage",
        ).increment(1);

        return Err(ApiError::BadRequest(EvidenceValidationError {
            errors: vec![error],
            warnings: vec![],
        }));
    }

    // 3. Store the validated envelope
    let envelope_id = store_evidence_envelope(&state.db_pool, &envelope).await?;

    // 4. Log successful ingestion
    tracing::info!(
        envelope_id = envelope_id,
        tenant_id = %envelope.tenant_id,
        scope = ?envelope.scope,
        "Evidence envelope stored successfully"
    );

    metrics::counter!(
        "evidence_envelope_accepted_total",
        "tenant_id" => envelope.tenant_id.clone(),
        "scope" => format!("{:?}", envelope.scope),
    ).increment(1);

    Ok(Json(SubmitEvidenceResponse {
        envelope_id,
        warnings: validation.warnings,
    }))
}

/// Validate chain linkage against current chain state.
async fn validate_chain_linkage(
    pool: &SqlitePool,
    envelope: &EvidenceEnvelope,
) -> Result<Option<IngestionError>, sqlx::Error> {
    let tail = get_evidence_chain_tail(pool, &envelope.tenant_id, envelope.scope).await?;

    match (&tail, &envelope.previous_root) {
        // First envelope: must have no previous_root
        (None, Some(claimed)) => {
            Ok(Some(IngestionError::ChainLinkageInvalid {
                reason: format!(
                    "First envelope in chain cannot have previous_root (claimed: {})",
                    claimed.to_short_hex()
                ),
            }))
        }

        // Subsequent envelope: must reference tail root
        (Some((tail_root, _)), None) => {
            Ok(Some(IngestionError::PreviousRootMismatch {
                expected: Some(*tail_root),
                claimed: None,
            }))
        }

        (Some((tail_root, _)), Some(claimed)) if tail_root != claimed => {
            Ok(Some(IngestionError::PreviousRootMismatch {
                expected: Some(*tail_root),
                claimed: Some(*claimed),
            }))
        }

        // Valid: first envelope with no previous, or correct linkage
        _ => Ok(None),
    }
}

/// Categorize errors for metrics labeling.
fn categorize_error(errors: &[IngestionError]) -> String {
    if errors.iter().any(|e| matches!(e, IngestionError::RootMismatch { .. })) {
        "root_mismatch"
    } else if errors.iter().any(|e| matches!(e, IngestionError::SignatureInvalid { .. })) {
        "signature_invalid"
    } else if errors.iter().any(|e| matches!(e, IngestionError::PayloadMissing { .. })) {
        "payload_missing"
    } else {
        "other"
    }
}

#[derive(Debug, Serialize)]
pub struct SubmitEvidenceResponse {
    pub envelope_id: i64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct EvidenceValidationError {
    pub errors: Vec<IngestionError>,
    pub warnings: Vec<String>,
}
```

#### 3. `crates/adapteros-db/src/evidence_envelopes.rs`

**Update store function to assume pre-validation**:

```rust
/// Store a pre-validated evidence envelope.
///
/// # Precondition
///
/// The envelope MUST have been validated via `validate_for_ingestion()`
/// before calling this function. This function only performs chain
/// sequence assignment, not validation.
///
/// # Errors
///
/// Returns error if DB write fails or chain sequence cannot be assigned.
pub async fn store_evidence_envelope(
    pool: &SqlitePool,
    envelope: &EvidenceEnvelope,
) -> Result<i64, DbError> {
    // Get current chain tail for sequence assignment
    let tail = get_evidence_chain_tail(pool, &envelope.tenant_id, envelope.scope).await?;
    let chain_sequence = tail.map(|(_, seq)| seq + 1).unwrap_or(1);

    // Insert envelope
    let result = sqlx::query(r#"
        INSERT INTO evidence_envelopes (
            schema_version, tenant_id, scope, previous_root, root,
            signature, public_key, key_id, attestation_ref,
            created_at, signed_at_us, chain_sequence,
            bundle_metadata_ref, policy_audit_ref, inference_receipt_ref
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    "#)
    .bind(envelope.schema_version)
    .bind(&envelope.tenant_id)
    .bind(envelope.scope.as_str())
    .bind(envelope.previous_root.map(|h| h.to_hex()))
    .bind(envelope.root.to_hex())
    .bind(&envelope.signature)
    .bind(&envelope.public_key)
    .bind(&envelope.key_id)
    .bind(envelope.attestation_ref.as_ref().map(|r| serde_json::to_string(r).ok()).flatten())
    .bind(&envelope.created_at)
    .bind(envelope.signed_at_us)
    .bind(chain_sequence)
    .bind(envelope.bundle_metadata_ref.as_ref().map(|r| serde_json::to_string(r).ok()).flatten())
    .bind(envelope.policy_audit_ref.as_ref().map(|r| serde_json::to_string(r).ok()).flatten())
    .bind(envelope.inference_receipt_ref.as_ref().map(|r| serde_json::to_string(r).ok()).flatten())
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}
```

#### 4. Add API route

**File**: `crates/adapteros-server-api/src/routes.rs`

```rust
pub fn evidence_routes(state: AppState) -> Router {
    Router::new()
        .route("/v1/evidence/submit", post(handlers::evidence::submit_evidence_envelope))
        .route("/v1/evidence/:envelope_id", get(handlers::evidence::get_evidence_envelope))
        .route("/v1/evidence/chain/:tenant_id/:scope", get(handlers::evidence::get_evidence_chain))
        .with_state(state)
}
```

---

## Acceptance Criteria

- [ ] Envelope root recomputed and verified before storage
- [ ] Root mismatch returns `400 Bad Request` with `RootMismatch` error
- [ ] Signature verified before storage (when `require_signatures = true`)
- [ ] Invalid signature returns `400 Bad Request` with `SignatureInvalid` error
- [ ] Chain linkage verified: `previous_root` must match chain tail
- [ ] `previous_root` mismatch returns `400 Bad Request` with `PreviousRootMismatch`
- [ ] Invalid envelopes rejected, not stored
- [ ] `audit_chain_divergence_event()` emitted on rejection
- [ ] Prometheus metrics track accepted/rejected counts by error type
- [ ] Successful submission returns `envelope_id` and any warnings

---

## Test Plan

### Unit Tests

**File**: `crates/adapteros-core/tests/evidence_ingestion_tests.rs`

```rust
#[test]
fn test_validate_valid_envelope() {
    let envelope = create_valid_telemetry_envelope();

    let result = validate_for_ingestion(&envelope, false);

    assert!(result.is_valid);
    assert!(result.errors.is_empty());
}

#[test]
fn test_validate_root_mismatch() {
    let mut envelope = create_valid_telemetry_envelope();
    // Tamper with root
    envelope.root = B3Hash::hash(b"wrong");

    let result = validate_for_ingestion(&envelope, false);

    assert!(!result.is_valid);
    assert!(result.errors.iter().any(|e| matches!(e, IngestionError::RootMismatch { .. })));
}

#[test]
fn test_validate_missing_payload() {
    let mut envelope = create_valid_telemetry_envelope();
    envelope.bundle_metadata_ref = None;

    let result = validate_for_ingestion(&envelope, false);

    assert!(!result.is_valid);
    assert!(result.errors.iter().any(|e| matches!(e, IngestionError::PayloadMissing { .. })));
}

#[test]
fn test_validate_scope_mismatch() {
    let mut envelope = create_valid_telemetry_envelope();
    envelope.scope = EvidenceScope::Inference; // Wrong scope

    let result = validate_for_ingestion(&envelope, false);

    assert!(!result.is_valid);
    assert!(result.errors.iter().any(|e| matches!(e, IngestionError::PayloadScopeMismatch { .. })));
}

#[cfg(feature = "evidence-signing")]
#[test]
fn test_validate_signature_required() {
    let mut envelope = create_valid_telemetry_envelope();
    envelope.signature = String::new();

    let result = validate_for_ingestion(&envelope, true);

    assert!(!result.is_valid);
    assert!(result.errors.iter().any(|e| matches!(e, IngestionError::SignatureMissing)));
}

#[cfg(feature = "evidence-signing")]
#[test]
fn test_validate_invalid_signature() {
    let mut envelope = create_signed_envelope();
    envelope.signature = hex::encode([0u8; 64]); // Wrong signature

    let result = validate_for_ingestion(&envelope, true);

    assert!(!result.is_valid);
    assert!(result.errors.iter().any(|e| matches!(e, IngestionError::SignatureInvalid { .. })));
}

#[test]
fn test_validate_key_id_mismatch() {
    let mut envelope = create_valid_telemetry_envelope();
    envelope.public_key = hex::encode([1u8; 32]);
    envelope.key_id = "wrong_key_id".to_string();

    let result = validate_for_ingestion(&envelope, false);

    assert!(!result.is_valid);
    assert!(result.errors.iter().any(|e| matches!(e, IngestionError::KeyIdMismatch { .. })));
}

#[test]
fn test_validate_timestamp_in_future() {
    let mut envelope = create_valid_telemetry_envelope();
    let future = chrono::Utc::now() + chrono::Duration::hours(1);
    envelope.created_at = future.to_rfc3339();

    let result = validate_for_ingestion(&envelope, false);

    assert!(!result.is_valid);
    assert!(result.errors.iter().any(|e| matches!(e, IngestionError::TimestampInFuture { .. })));
}
```

### Integration Tests

**File**: `tests/evidence_submission_integration.rs`

```rust
#[tokio::test]
async fn test_submit_valid_envelope() {
    let server = start_test_server().await;
    let envelope = create_valid_telemetry_envelope();

    let response = server.post("/v1/evidence/submit")
        .json(&envelope)
        .send()
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: SubmitEvidenceResponse = response.json().await;
    assert!(body.envelope_id > 0);
}

#[tokio::test]
async fn test_submit_tampered_envelope_rejected() {
    let server = start_test_server().await;
    let mut envelope = create_valid_telemetry_envelope();
    envelope.root = B3Hash::hash(b"tampered");

    let response = server.post("/v1/evidence/submit")
        .json(&envelope)
        .send()
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: EvidenceValidationError = response.json().await;
    assert!(body.errors.iter().any(|e| matches!(e, IngestionError::RootMismatch { .. })));
}

#[tokio::test]
async fn test_chain_linkage_enforced() {
    let server = start_test_server().await;

    // Submit first envelope
    let envelope1 = create_valid_telemetry_envelope();
    server.post("/v1/evidence/submit").json(&envelope1).send().await;

    // Submit second envelope with wrong previous_root
    let mut envelope2 = create_valid_telemetry_envelope();
    envelope2.previous_root = Some(B3Hash::hash(b"wrong"));

    let response = server.post("/v1/evidence/submit")
        .json(&envelope2)
        .send()
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: EvidenceValidationError = response.json().await;
    assert!(body.errors.iter().any(|e| matches!(e, IngestionError::PreviousRootMismatch { .. })));
}

#[tokio::test]
async fn test_rejection_emits_observability_event() {
    let server = start_test_server().await;
    let mut envelope = create_valid_telemetry_envelope();
    envelope.root = B3Hash::hash(b"tampered");

    server.post("/v1/evidence/submit")
        .json(&envelope)
        .send()
        .await;

    // Check metrics include rejection
    let metrics = fetch_metrics(&server).await;
    assert!(metrics.contains("evidence_envelope_rejected_total"));
}
```

---

## Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `evidence_envelope_accepted_total` | Counter | tenant_id, scope | Successfully stored envelopes |
| `evidence_envelope_rejected_total` | Counter | tenant_id, scope, error_type | Rejected envelopes by error |
| `evidence_validation_duration_ms` | Histogram | - | Time spent validating envelopes |

---

## Configuration

**File**: `configs/cp.toml`

```toml
[evidence]
# Require valid signatures on all submitted evidence envelopes
require_signatures = true

# Maximum clock skew allowed for envelope timestamps (seconds)
max_timestamp_skew_secs = 300
```
