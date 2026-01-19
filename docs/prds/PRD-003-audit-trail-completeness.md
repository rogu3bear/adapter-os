# PRD-003: Audit Trail Completeness - Blocking TODOs

**Status**: Completed
**Priority**: P0 (Production Blocker)
**Estimated Effort**: 8-12 hours
**Owner**: TBD
**Completed**: 2026-01-19

---

## 1. Problem Statement

The cryptographic audit trail system has three blocking gaps that violate the core promise of deterministic, auditable inference:

1. **Cancelled inference requests generate no receipt** - Partial outputs are unverifiable
2. **28 boot invariants are not validated** - System can start in invalid security state
3. **RAG evidence lacks message_id binding** - Evidence cannot be traced to originating message

These gaps mean:
- Auditors cannot verify all inference outcomes
- Security invariants may be violated without detection
- Evidence chain has broken links in chat flows

---

## 2. Scope

### In Scope

| Issue | File | Impact |
|-------|------|--------|
| Cancel receipt generation | `adapteros-lora-worker/src/lib.rs:1926` | Audit gap |
| Boot invariants (28 items) | `adapteros-server/src/boot/invariants.rs:420,558` | Security gap |
| RAG message_id binding | `adapteros-server-api/src/handlers/streaming_infer.rs:805`, `inference_core/core.rs:2601` | Trace gap |

### Out of Scope

- Tenant metrics (separate PRD)
- ETag generation (separate PRD)
- Diagnostic bundle completeness (separate PRD)

---

## 3. Technical Analysis

### 3.1 Cancel Receipt Generation

**Current State**: When inference is cancelled (client disconnect, timeout, explicit cancel), the worker logs the cancellation but generates no cryptographic receipt.

**Location**: `crates/adapteros-lora-worker/src/lib.rs:1926`

```rust
// Current code (line 1926)
// TODO: Generate error receipt with partial output using StopReasonCode::Cancelled
```

**Required Behavior**:

1. Capture partial output tokens generated before cancellation
2. Generate receipt with `StopReasonCode::Cancelled`
3. Include cancellation metadata (reason, token index, timestamp)
4. Sign receipt with worker keypair
5. Store receipt for audit retrieval

**Receipt Schema Extension**:

```rust
pub struct CancellationReceipt {
    pub trace_id: String,
    pub partial_output_digest: B3Hash,      // BLAKE3 of tokens generated
    pub partial_output_count: u32,          // Tokens before cancel
    pub stop_reason: StopReasonCode,        // Cancelled
    pub cancellation_source: CancelSource,  // Client, Timeout, Policy, Manual
    pub cancelled_at_token: u32,            // Token index at cancellation
    pub receipt_digest: B3Hash,             // Final receipt hash
    pub signature: Ed25519Signature,        // Worker signature
}

pub enum CancelSource {
    ClientDisconnect,
    RequestTimeout,
    PolicyViolation,
    ManualCancel,
    ResourceExhaustion,
}
```

**Implementation Steps**:

1. Add `CancellationReceipt` to `adapteros-core/src/crypto_receipt.rs`
2. In cancel handler, collect partial output state
3. Compute receipt digest over partial state
4. Sign with worker keypair (fail-closed in production)
5. Store via `TraceSink::finalize_cancelled()`
6. Return receipt in cancel response

### 3.2 Boot Invariants (28 Remaining)

**Current State**: The boot sequence documents 28 security invariants that should be validated, but only logs them as TODOs.

**Location**: `crates/adapteros-server/src/boot/invariants.rs:420,558`

```rust
// Line 420
// TODO: Remaining invariants to implement (28 more from analysis)

// Line 558
// TODO: Implement post-DB invariants (trigger checks, etc.)
```

**Invariant Categories**:

| Category | Count | Examples |
|----------|-------|----------|
| Authentication | 5 | JWT key loaded, HMAC secret non-default, session store initialized |
| Authorization | 4 | RBAC tables populated, default roles exist, admin role defined |
| Cryptographic | 6 | Worker keypair exists, signing key valid, entropy source available |
| Database | 5 | Migrations complete, triggers exist, indexes valid |
| Federation | 3 | Quorum keys loaded (if federated), peer certs valid |
| Adapters | 3 | Bundle signatures verified, manifest hashes match |
| Policy | 2 | Default policy pack loaded, enforcement mode set |

**Implementation Approach**:

Create invariant check functions that return `Result<(), InvariantViolation>`:

```rust
pub struct InvariantViolation {
    pub invariant_id: &'static str,
    pub category: InvariantCategory,
    pub message: String,
    pub severity: Severity,
    pub remediation: Option<String>,
}

pub enum Severity {
    Fatal,      // Abort boot
    Error,      // Log and continue with degraded mode
    Warning,    // Log only
}

// Example invariant
fn check_jwt_signing_key(state: &BootState) -> Result<(), InvariantViolation> {
    let key = state.jwt_signing_key.as_ref().ok_or_else(|| {
        InvariantViolation {
            invariant_id: "AUTH-001",
            category: InvariantCategory::Authentication,
            message: "JWT signing key not configured".into(),
            severity: Severity::Fatal,
            remediation: Some("Set JWT_SIGNING_KEY environment variable".into()),
        }
    })?;

    // Validate key format
    if key.len() < 32 {
        return Err(InvariantViolation {
            invariant_id: "AUTH-002",
            category: InvariantCategory::Authentication,
            message: "JWT signing key too short (minimum 32 bytes)".into(),
            severity: Severity::Fatal,
            remediation: Some("Generate a secure key: openssl rand -base64 32".into()),
        });
    }

    Ok(())
}
```

**Boot Phase Integration**:

```rust
pub async fn run_invariant_checks(state: &BootState) -> Result<InvariantReport, BootError> {
    let mut report = InvariantReport::new();

    // Pre-DB invariants (can run before database connection)
    report.check("AUTH-001", check_jwt_signing_key(state));
    report.check("AUTH-002", check_hmac_secret_not_default(state));
    report.check("CRYPTO-001", check_worker_keypair_exists(state));
    // ... 25 more

    // Post-DB invariants (require database connection)
    report.check("DB-001", check_migrations_complete(&state.db).await);
    report.check("DB-002", check_audit_triggers_exist(&state.db).await);
    // ... more

    // Fail boot if any Fatal invariants failed
    if report.has_fatal_violations() {
        return Err(BootError::InvariantViolations(report));
    }

    Ok(report)
}
```

### 3.3 RAG Evidence Message ID Binding

**Current State**: When RAG retrieval happens during inference, the evidence is stored without binding to the message_id that will contain the response.

**Locations**:
- `crates/adapteros-server-api/src/handlers/streaming_infer.rs:805`
- `crates/adapteros-server-api/src/inference_core/core.rs:2601`

```rust
// Line 805 (streaming_infer.rs)
// TODO: Pass message_id when chat flow creates it before inference

// Line 2601 (core.rs)
// TODO: Pass message_id when chat flow creates it before inference
```

**Problem**: In chat flows, the message_id is generated after inference starts. RAG evidence stored before message_id exists cannot be linked back.

**Solution**: Two-phase evidence binding:

```rust
// Phase 1: Store evidence with placeholder
pub struct PendingEvidence {
    pub evidence_id: Uuid,
    pub trace_id: String,
    pub rag_results: Vec<RagResult>,
    pub stored_at: DateTime<Utc>,
    pub message_id: Option<String>,  // Initially None
}

// Phase 2: Bind message_id after generation
pub async fn bind_evidence_to_message(
    db: &Db,
    evidence_id: Uuid,
    message_id: &str,
) -> Result<(), DbError> {
    sqlx::query!(
        "UPDATE rag_evidence SET message_id = ? WHERE evidence_id = ?",
        message_id,
        evidence_id.to_string()
    )
    .execute(db.pool())
    .await?;

    Ok(())
}
```

**Integration Points**:

1. In `streaming_infer.rs`: After RAG retrieval, store evidence with `evidence_id`
2. Pass `evidence_id` through inference pipeline
3. After message generation, call `bind_evidence_to_message()`
4. In non-streaming `core.rs`: Same pattern

**Verification Query**:

```sql
-- Find unbound evidence (should be empty in healthy system)
SELECT evidence_id, trace_id, stored_at
FROM rag_evidence
WHERE message_id IS NULL
AND stored_at < NOW() - INTERVAL '5 minutes';
```

---

## 4. Implementation Plan

### Phase 1: Cancel Receipt Generation (3-4 hours)

1. Define `CancellationReceipt` struct in `adapteros-core`
2. Add `CancelSource` enum
3. Implement `ReceiptGenerator::finalize_cancelled()`
4. Update cancel handlers to generate receipts
5. Add `TraceSink::store_cancellation_receipt()`
6. Write tests for cancel receipt verification

### Phase 2: Boot Invariants (4-6 hours)

1. Create `invariants/` module with check functions
2. Define all 28 invariant checks (stub implementations initially)
3. Implement Fatal invariants first (authentication, crypto)
4. Implement Error invariants (database, policy)
5. Integrate into boot sequence
6. Add `/readyz` endpoint integration for invariant status

### Phase 3: RAG Evidence Binding (2-3 hours)

1. Add `message_id` column to `rag_evidence` table (migration)
2. Create `PendingEvidence` tracking
3. Implement `bind_evidence_to_message()`
4. Update streaming inference to track `evidence_id`
5. Update non-streaming inference
6. Add monitoring query for unbound evidence

---

## 5. Database Migrations

### Migration: Add message_id to rag_evidence

```sql
-- migrations/NNNN_add_message_id_to_rag_evidence.sql
ALTER TABLE rag_evidence ADD COLUMN message_id TEXT;
CREATE INDEX idx_rag_evidence_message_id ON rag_evidence(message_id);

-- Backfill: Mark existing evidence as legacy (no binding possible)
UPDATE rag_evidence SET message_id = '__legacy_unbound__' WHERE message_id IS NULL;
```

### Migration: Add cancellation_receipts table

```sql
-- migrations/NNNN_add_cancellation_receipts.sql
CREATE TABLE cancellation_receipts (
    id TEXT PRIMARY KEY,
    trace_id TEXT NOT NULL,
    partial_output_digest BLOB NOT NULL,
    partial_output_count INTEGER NOT NULL,
    stop_reason TEXT NOT NULL,
    cancellation_source TEXT NOT NULL,
    cancelled_at_token INTEGER NOT NULL,
    receipt_digest BLOB NOT NULL,
    signature BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (trace_id) REFERENCES inference_traces(trace_id)
);
CREATE INDEX idx_cancellation_receipts_trace_id ON cancellation_receipts(trace_id);
```

---

## 6. Acceptance Criteria

### Cancel Receipts

- [x] Cancelled inference generates signed receipt
- [x] Receipt includes partial output digest
- [ ] Receipt verifiable via `aosctl verify-receipt` (CLI integration pending)
- [x] Cancel receipts queryable via API

### Boot Invariants

- [x] All 28 invariants have check functions (31 implemented)
- [x] Fatal violations abort boot with clear error
- [x] Boot logs invariant check results
- [x] `/readyz` reflects invariant status

### RAG Evidence Binding

- [x] New evidence includes `message_id` when available
- [x] Binding happens within same transaction as message storage
- [x] Unbound evidence older than 5 minutes triggers alert (`get_unbound_evidence` method)
- [x] Legacy evidence marked as `__legacy_unbound__` (migration backfills)

---

## 7. Testing Strategy

### Cancel Receipt Tests

```rust
#[tokio::test]
async fn test_cancel_generates_receipt() {
    let worker = TestWorker::new();
    let request = InferRequest::new("Hello").with_max_tokens(100);

    // Start inference
    let handle = worker.start_inference(request);

    // Wait for some tokens
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Cancel
    handle.cancel().await;

    // Verify receipt exists
    let receipt = worker.get_receipt(handle.trace_id()).await.unwrap();
    assert_eq!(receipt.stop_reason, StopReasonCode::Cancelled);
    assert!(receipt.partial_output_count > 0);

    // Verify signature
    assert!(verify_receipt_signature(&receipt, &worker.public_key()).is_ok());
}
```

### Boot Invariant Tests

```rust
#[test]
fn test_boot_fails_without_jwt_key() {
    let config = BootConfig::default(); // No JWT key
    let result = run_invariant_checks(&config);

    assert!(result.is_err());
    let violations = result.unwrap_err().violations();
    assert!(violations.iter().any(|v| v.invariant_id == "AUTH-001"));
}
```

### RAG Evidence Binding Tests

```rust
#[tokio::test]
async fn test_rag_evidence_bound_to_message() {
    let db = TestDb::new();
    let chat = ChatSession::new(&db);

    // Send message with RAG
    let response = chat.send_message("What is X?").await;

    // Check evidence binding
    let evidence = db.get_evidence_for_message(&response.message_id).await;
    assert!(!evidence.is_empty());
    assert!(evidence.iter().all(|e| e.message_id == Some(response.message_id.clone())));
}
```

---

## 8. Monitoring & Alerting

### Metrics

```rust
// Cancel receipt generation
counter!("aos.receipts.cancellation.generated").increment(1);
counter!("aos.receipts.cancellation.failed").increment(1);

// Boot invariants
gauge!("aos.boot.invariants.passed").set(passed_count);
gauge!("aos.boot.invariants.failed").set(failed_count);

// Evidence binding
counter!("aos.evidence.bound").increment(1);
counter!("aos.evidence.unbound").increment(1);  // Alert if > 0
```

### Alerts

| Alert | Condition | Severity |
|-------|-----------|----------|
| `CancelReceiptFailure` | Cancel receipt generation fails | P1 |
| `BootInvariantViolation` | Any invariant fails at boot | P0 |
| `UnboundEvidence` | Evidence unbound > 5 min | P2 |

---

## 9. Success Metrics

| Metric | Before | After | Target |
|--------|--------|-------|--------|
| Cancel receipt coverage | 0% | 100% | 100% |
| Boot invariants validated | 0/28 | 28/28 | 28/28 |
| Evidence binding rate | ~70% | 100% | 100% |
| Audit trail completeness | Partial | Full | Full |

---

## 10. Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Cancel receipt adds latency | Medium | Low | Async receipt storage, don't block cancel response |
| Boot invariants too strict | Medium | Medium | Start with Warning severity, promote to Fatal after validation |
| Evidence binding race condition | Low | Medium | Use database transaction for atomic binding |

---

## 11. Dependencies

- PRD-001 must complete first (ensure examples compile for testing)
- Database migration tooling must be functional
- Worker keypair must exist for receipt signing
