# Threat Model & Hardening Plan: Receipts and Audit Trails

**Version:** 1.0
**Date:** 2026-01-06
**Scope:** Tamper-evidence and forensic replay for adapterOS inference receipts and policy audit chains

---

## Threat Model

### Attacker Profiles

#### 1. Malicious Tenant/User

**Capabilities:**
- Submit arbitrary inference requests
- Attempt to replay or forge receipts for billing/audit manipulation
- Attempt to modify their own policy audit decisions after the fact
- Access their own telemetry bundles and attempt to tamper

**What they CAN tamper with:**
- Their own request payloads before submission
- Local copies of receipts after download
- Claims about what receipts they received

**What they CANNOT tamper with (assumptions):**
- Server-side receipt storage (database isolation)
- Other tenants' data (tenant isolation enforced)
- Ed25519 signing keys (HSM/secure storage)
- Policy audit chain entries once written (append-only)

**Detection goals:**
- Detect forged/modified receipts via signature verification
- Detect replayed receipts via trace_id uniqueness
- Detect billing manipulation via receipt_digest covering billing fields

**Prevention goals:**
- Prevent receipt forgery (Ed25519 signatures)
- Prevent cross-tenant data access (tenant isolation)

---

#### 2. Compromised Worker Process

**Capabilities:**
- Execute inference with access to model weights, seeds, and adapters
- Generate receipts and token decisions
- Access thread-local seed context during request processing
- Write telemetry events

**What they CAN tamper with:**
- Token decisions and routing choices during inference
- Seed derivation if they can inject into thread-local context
- Telemetry events before they're bundled
- Receipt fields before signing (if signing happens in worker)

**What they CANNOT tamper with (assumptions):**
- Signing keys (should be in control plane, not worker)
- Already-written database records
- Already-signed telemetry bundles
- Policy audit chain in control plane database

**Detection goals:**
- Detect seed substitution via `root_seed_digest_hex` in V3 receipts
- Detect output drift via `output_digest_hex` verification
- Detect routing manipulation via `run_head_hash_hex` chain

**Prevention goals:**
- Move receipt signing to control plane (not worker)
- Enforce seed context isolation via middleware
- Validate worker outputs against determinism constraints

---

#### 3. Compromised Control Plane

**Capabilities:**
- Full database access (read/write/delete)
- Access to signing keys
- Modify receipts before/after storage
- Modify policy audit chain entries
- Modify telemetry bundle metadata

**What they CAN tamper with:**
- All database records including receipts
- Policy audit chain (break links, modify entries)
- Telemetry bundle signatures
- Signing key rotation

**What they CANNOT tamper with (assumptions):**
- Externally exported and independently verified receipts
- Telemetry bundles already exported to external storage
- Client-side receipt copies with valid signatures

**Detection goals:**
- Detect chain breaks via periodic audit chain verification
- Detect receipt modification via external verifier comparison
- Detect bundle tampering via Merkle root verification

**Prevention goals:**
- Export receipts/bundles to immutable external storage
- Implement multi-party signing for high-value receipts
- Run verification from isolated/external systems

---

#### 4. Compromised Disk/Storage

**Capabilities:**
- Modify SQLite database files at rest
- Modify .aos adapter files
- Modify telemetry bundle files
- Modify configuration files

**What they CAN tamper with:**
- Any file on disk
- Database entries
- Adapter weights

**What they CANNOT tamper with (assumptions):**
- In-memory state during active operations
- Network-transmitted data already received by clients
- Cryptographic signatures (without keys)

**Detection goals:**
- Detect database tampering via dual-write divergence (SQL vs KV)
- Detect .aos file tampering via segment hash verification
- Detect adapter tampering via `hash_b3` column validation
- Detect config tampering via policy hash baseline checks

**Prevention goals:**
- Enable dual-write mode for critical tables
- Verify hashes on every .aos file load
- Periodic integrity scans of adapter registry

---

#### 5. Honest But Buggy Backend

**Capabilities:**
- Produce non-deterministic outputs due to:
  - Floating-point rounding differences
  - Thread scheduling variations
  - Uninitialized memory reads
  - Seed propagation failures

**What they CAN cause:**
- Output drift between runs
- Receipt mismatches on replay
- Seed context leakage between requests
- Router decision divergence

**What they CANNOT cause:**
- Signature forgery
- Chain link breaks (those are data integrity issues)

**Detection goals:**
- Detect output drift via replay verification
- Detect seed leakage via `seed_fallback_used_total` metric
- Detect router divergence via determinism test suite
- Detect floating-point issues via Q15 quantization checks

**Prevention goals:**
- Enforce `-fno-fast-math` in all builds
- Require seed context in production (`AOS_REQUIRE_SEED_CONTEXT=1`)
- Run determinism verification in CI
- Use `SeedScopeGuard` RAII pattern at all boundaries

---

## Integrity Surfaces Inventory

### 1. Model Hashes

| Artifact | Produced | Stored | Validated | Gap |
|----------|----------|--------|-----------|-----|
| `model.hash_b3` | Model registration (`aosctl models seed`) | `models.hash_b3` column | On model load | **None** - verified on load |
| `config_hash_b3` | Model registration | `models.config_hash_b3` | On model load | **None** |
| `tokenizer_hash_b3` | Model registration | `models.tokenizer_hash_b3` | `aosctl models check-tokenizer` | **Gap**: Not auto-verified on inference |
| `tokenizer_cfg_hash_b3` | Model registration | `models.tokenizer_cfg_hash_b3` | On model load | **None** |

**Source:** `crates/adapteros-db/src/models.rs`, `migrations/0001_init.sql`

---

### 2. Adapter Hashes / .AOS File Hashes

| Artifact | Produced | Stored | Validated | Gap |
|----------|----------|--------|-----------|-----|
| `adapters.hash_b3` | Adapter registration | `adapters.hash_b3` (unique) | On adapter load | **None** |
| `aos_file_hash` | .AOS write (`AosWriter`) | `aos_adapter_metadata.aos_file_hash` | On .AOS open | **None** - segment hashes verified |
| Per-segment `weights_hash` | `AosWriter::add_segment()` | Embedded in .AOS index | `open_aos()` re-hashes payload | **None** |
| `scope_hash` (16 bytes) | `compute_scope_hash()` | .AOS index entry | On segment lookup | **None** |

**Source:** `crates/adapteros-aos/src/writer.rs`, `crates/adapteros-aos/src/implementation.rs`

---

### 3. Router Decision Hash (run_head_hash)

| Artifact | Produced | Stored | Validated | Gap |
|----------|----------|--------|-----------|-----|
| Per-token `decision_hash_hex` | `hash_decision()` during inference | `ReceiptToken.decision_hash_hex` | `verify_receipt` recomputes | **None** |
| `run_head_hash_hex` | Chained via `update_head()` | `ReceiptDigests.run_head_hash_hex` | `verify_receipt` recomputes chain | **None** |
| `context_digest_hex` | `compute_context_digest()` | `ReceiptBundle.context_digest_hex` | `verify_receipt` recomputes | **None** |

**Source:** `crates/adapteros-cli/src/commands/verify_receipt.rs` (lines 200-280)

---

### 4. Receipts and Signatures

| Artifact | Produced | Stored | Validated | Gap |
|----------|----------|--------|-----------|-----|
| `receipt_digest_hex` | `compute_receipt_digest()` | `ReceiptDigests.receipt_digest_hex` | `verify_receipt` recomputes | **Closed** (V7 parity) |
| `signature_b64` | Ed25519 signing | `ReceiptDigests.signature_b64` | `verify_signature()` | **Gap**: Signing location unclear |
| `public_key_hex` | Key generation | `ReceiptDigests.public_key_hex` | Signature verification | **Closed**: `receipt_signing_kid` + `receipt_signed_at` enable rotation audit (V7) |
| Schema version (V1–V7) | Receipt creation | `ReceiptDigests.schema_version` | Version-aware digest computation | **None** |

**CRITICAL GAP (Closed in V7):**
The CLI verification now includes V4–V7 receipt fields (stop controller, KV quota, prefix cache, model cache, equipment profile, cross-run lineage, and V7 bindings), restoring parity with production receipts.

---

### 5. Policy Audit Chain Entries

| Artifact | Produced | Stored | Validated | Gap |
|----------|----------|--------|-----------|-----|
| `entry_hash` | `compute_policy_entry_hash()` | `policy_audit_decisions.entry_hash` | `verify_policy_audit_chain()` | **None** |
| `previous_hash` | From tail validation | `policy_audit_decisions.previous_hash` | Chain linkage verification | **None** |
| `chain_sequence` | Monotonic increment | `policy_audit_decisions.chain_sequence` | Sequence continuity check | **None** |

**Source:** `crates/adapteros-db/src/policy_audit.rs` (lines 105-123, 428-626)

**Gap:** No scheduled/periodic verification job. Verification only runs on-demand via API.

---

### 6. Telemetry Bundles and Merkle Roots

| Artifact | Produced | Stored | Validated | Gap |
|----------|----------|--------|-----------|-----|
| `merkle_root` | `compute_merkle_root()` | `.ndjson.sig` metadata | `verify_bundle()` | **None** |
| `signature` | Ed25519 over merkle_root | `.ndjson.sig` metadata | Bundle verification | **None** |
| `prev_bundle_hash` | Previous bundle's merkle_root | `BundleMetadata.prev_bundle_hash` | Chain verification | **Gap**: No scheduled verification |
| Per-event `hash` | BLAKE3 of event JSON | `TelemetryEvent.hash` | Merkle leaf verification | **Gap**: Optional field |

**Source:** `crates/adapteros-telemetry/src/merkle.rs`, `crates/adapteros-telemetry/src/bundle.rs`

---

### 7. Seed Lineage (V3 Receipts)

| Artifact | Produced | Stored | Validated | Gap |
|----------|----------|--------|-----------|-----|
| `root_seed_digest_hex` | BLAKE3 of derived seed | `ReceiptDigests.root_seed_digest_hex` | V3 digest includes it | **Gap**: Only in V3 |
| `seed_mode` | From `SeedContext` | `ReceiptDigests.seed_mode` | Mode validation | **Gap**: Only in V3 |
| `has_manifest_binding` | From `SeedLineage` | `ReceiptDigests.has_manifest_binding` | Binding check | **Gap**: Only in V3 |

**Source:** `crates/adapteros-core/src/seed.rs` (lines 325-418)

---

## Receipt Completeness Recommendations

### Fields That SHOULD Be Covered by `receipt_digest`

The following fields should be included in receipt digest computation to ensure tamper-evidence:

#### Tier 1: Already Covered (V1-V3)
- `context_digest_hex` - Input determinism
- `run_head_hash_hex` - Router decision chain
- `output_digest_hex` - Output integrity
- `logical_prompt_tokens`, `billed_input_tokens` - Billing accuracy
- `logical_output_tokens`, `billed_output_tokens` - Billing accuracy
- `prefix_cached_token_count` - Cache accounting

#### Tier 2: Covered in V2+ Only
- `backend_used` - Hardware binding
- `backend_attestation_b3_hex` - Backend determinism proof

#### Tier 3: Covered in V3 Only
- `root_seed_digest_hex` - Seed lineage (NOT raw seed - privacy safe)
- `seed_mode` - Strict/BestEffort/NonDeterministic
- `has_manifest_binding` - Manifest was used

#### Tier 4: MISSING - Should Be Added (Proposed V4)

| Field | Rationale | Privacy | Size Impact |
|-------|-----------|---------|-------------|
| `stop_reason_code` | Completeness verification | Safe | +~10 bytes |
| `stop_reason_token_index` | Output boundary proof | Safe | +4 bytes |
| `kv_residency_policy_id` | Cache policy binding | Safe | +~40 bytes |
| `kv_quota_enforced` | Quota enforcement proof | Safe | +1 byte |
| `model_cache_identity_v2_digest_b3` | Model version binding | Safe | +32 bytes |
| `policy_mask_digest_hex` | Policy enforcement proof | Safe | Already in context |
| `stack_hash_hex` | Adapter stack binding | Safe | Already in context |
| `kernel_plan_hash_b3` | Kernel version binding | Safe | +32 bytes |
| `metallib_hash_b3` | Metal shader binding | Safe | +32 bytes (macOS only) |

**Total additional size:** ~150 bytes per receipt (acceptable)

#### Versioning Strategy

```rust
pub const RECEIPT_SCHEMA_VERSION: u8 = 4;  // Increment for V4

// Backward compatibility
match schema_version {
    1 => compute_v1_digest(...),  // Original fields only
    2 => compute_v2_digest(...),  // + backend binding
    3 => compute_v3_digest(...),  // + seed lineage
    4 => compute_v4_digest(...),  // + stop/kv/kernel/metal
    _ => Err("Unsupported schema version"),
}
```

#### Fields That Should NOT Be Included

| Field | Reason |
|-------|--------|
| Raw seed bytes | Privacy/security - use digest instead |
| Private signing keys | Security |
| Full telemetry events | Size - use Merkle root reference |
| User PII | Privacy |
| Worker internal state | Instability across versions |

---

## Verification Workflows

### 1. Local Receipt Verification (CLI)

**Command:**
```bash
./aosctl verify-receipt --bundle ./receipt.json [--strict]
```

**Workflow:**
```
1. Load ReceiptBundle from JSON
2. Detect schema_version from receipt.schema_version
3. Recompute digests based on version:
   a. compute_context_digest() from context fields
   b. For each token: hash_decision() + update_head()
   c. compute_output_digest() from output_tokens
   d. compute_receipt_digest() with ALL version-specific fields
4. Compare computed vs stored digests
5. If signature present:
   a. Decode signature_b64 and public_key_hex
   b. Ed25519 verify(receipt_digest, signature)
6. Report results with reason codes
```

**Failure Handling:**
- `CONTEXT_MISMATCH` → "Input context was modified"
- `TRACE_TAMPER` → "Token decision chain was modified"
- `OUTPUT_MISMATCH` → "Output tokens were modified"
- `SIGNATURE_INVALID` → "Receipt signature verification failed"
- `SCHEMA_VERSION_UNSUPPORTED` → "Unknown receipt schema version"

**Observability Events:**
- `ReceiptMismatch` on any digest mismatch
- `DeterminismViolation` on seed-related failures
- Log verification duration for performance tracking

---

### 2. Server-Side Verification on Ingestion

**Trigger:** When receipt is fetched from database for API response

**Workflow:**
```
1. Load receipt from inference_trace_receipts table
2. Verify receipt_digest matches recomputed value
3. Verify signature if present
4. If verification fails:
   a. Emit ReceiptMismatch observability event
   b. Return 409 CONFLICT with RECEIPT_VERIFICATION_FAILED
   c. Do NOT serve the receipt to client
5. If verification passes:
   a. Return receipt to client
   b. Include verification_status: "verified" in response
```

**Implementation Location:** `crates/adapteros-server-api/src/handlers/replay.rs`

---

### 3. Periodic Audit Chain Verification Job

**Proposed Schedule:** Every 15 minutes (configurable)

**Workflow:**
```
1. For each tenant:
   a. Call verify_policy_audit_chain(tenant_id)
   b. If divergence detected:
      - Emit AuditChainDivergence event
      - Increment audit_divergence_total counter
      - Alert via configured alerting channel
      - Log: tenant_id, first_invalid_sequence, error_message
   c. If successful:
      - Update last_verified_at timestamp
      - Log verification duration

2. For telemetry bundles:
   a. For each bundle in last 24 hours:
      - Verify merkle_root matches recomputed value
      - Verify prev_bundle_hash chain linkage
      - Verify signature
   b. If tampering detected:
      - Emit AuditExportTamper event
      - Mark bundle as quarantined
```

**Configuration:**
```toml
[audit_verification]
enabled = true
interval_seconds = 900  # 15 minutes
alert_on_divergence = true
verify_telemetry_bundles = true
bundle_lookback_hours = 24
```

**Observability Events:**
- `AuditChainDivergence` on chain verification failure
- `AuditExportTamper` on bundle tampering
- Metrics: `audit_verification_duration_seconds`, `audit_chain_entries_verified_total`

---

### 4. External Verifier (Air-Gapped)

**Command:**
```bash
./aosctl verify telemetry --bundle-dir ./diag_bundle/telemetry
```

**Workflow:**
```
1. Load all .ndjson and .ndjson.sig files
2. For each bundle:
   a. Parse events from NDJSON
   b. Recompute Merkle root
   c. Verify signature against merkle_root
   d. Verify prev_bundle_hash chain
3. Report:
   - Total bundles verified
   - Chain continuity status
   - Any signature failures
   - Any hash mismatches
```

---

## PR Plan

### PR 1: Fix verify_receipt.rs Digest Computation Mismatch

**Scope:** Align offline CLI verification with production receipt digest computation

**Code Targets:**
- `crates/adapteros-cli/src/commands/verify_receipt.rs` (lines 435-544)
- Add missing fields from `crates/adapteros-db/src/inference_trace.rs` (lines 270-342)

**Changes:**
1. Add `StopControllerFields` struct to `ReceiptDigests`:
   ```rust
   stop_reason_code: Option<String>,
   stop_reason_token_index: Option<u32>,
   stop_policy_digest_b3_hex: Option<String>,
   ```

2. Add `KvResidencyFields` struct:
   ```rust
   tenant_kv_quota_bytes: Option<u64>,
   tenant_kv_bytes_used: Option<u64>,
   kv_evictions: Option<u32>,
   kv_residency_policy_id: Option<String>,
   kv_quota_enforced: Option<bool>,
   ```

3. Add `PrefixCacheFields` struct:
   ```rust
   prefix_kv_key_b3_hex: Option<String>,
   prefix_cache_hit: Option<bool>,
   prefix_kv_bytes: Option<u64>,
   ```

4. Add `model_cache_identity_v2_digest_b3_hex: Option<String>`

5. Update `compute_receipt_digest()` to include all fields for V1-V3

**Acceptance Criteria:**
- [ ] Receipts generated by `inference_trace.rs` verify successfully with `verify_receipt.rs`
- [ ] All existing receipt JSON fixtures pass verification
- [ ] New test: `test_production_receipt_verifies_offline`
- [ ] Documentation updated with new fields

**Tests to Add:**
- `tests/receipt_verification_parity.rs` - Generate receipt via inference_trace, verify via CLI
- Unit tests for each new field's inclusion in digest

---

### PR 2: Implement Periodic Audit Chain Verification Job

**Scope:** Add background task that periodically verifies policy audit chain integrity

**Code Targets:**
- `crates/adapteros-server/src/background_tasks/mod.rs` (new module)
- `crates/adapteros-db/src/policy_audit.rs` (add `get_all_tenant_ids()`)
- `crates/adapteros-server-api/src/handlers/tenant_policies.rs` (emit events)

**Changes:**
1. Create `crates/adapteros-server/src/background_tasks/audit_verifier.rs`:
   ```rust
   pub struct AuditVerificationTask {
       db: Db,
       interval: Duration,
       alert_sender: Option<AlertSender>,
   }

   impl AuditVerificationTask {
       pub async fn run_once(&self) -> Result<AuditVerificationReport> {
           let tenant_ids = self.db.get_all_tenant_ids().await?;
           for tenant_id in tenant_ids {
               let result = self.db.verify_policy_audit_chain(Some(&tenant_id)).await?;
               if result.divergence_detected {
                   emit_audit_chain_divergence_event(&tenant_id, &result);
                   if let Some(ref sender) = self.alert_sender {
                       sender.send_alert(AuditDivergenceAlert { tenant_id, result }).await?;
                   }
               }
           }
           Ok(report)
       }
   }
   ```

2. Add configuration to `configs/cp.toml`:
   ```toml
   [audit_verification]
   enabled = true
   interval_seconds = 900
   ```

3. Integrate with server startup in `adapteros-server/src/main.rs`

**Acceptance Criteria:**
- [ ] Background task runs on configured interval
- [ ] `AuditChainDivergence` event emitted on detection
- [ ] `audit_divergence_total` metric incremented
- [ ] Graceful shutdown stops verification task
- [ ] Task is disabled by default in tests

**Tests to Add:**
- `tests/background_tasks/audit_verifier_test.rs`
- Integration test: inject corruption, verify detection
- Test: verify task respects shutdown signal

---

### PR 3: Harden Seed Context Requirement in Production

**Scope:** Prevent dev/test fallback seed behavior from entering production paths

**Code Targets:**
- `crates/adapteros-core/src/seed_override.rs` (lines 406-422, 466-503)
- `crates/adapteros-lora-worker/src/mlx_subprocess_bridge.rs`
- `crates/adapteros-server/src/middleware/seed_isolation.rs`

**Changes:**
1. Change default `require_seed_context` to `true` for ALL builds:
   ```rust
   // OLD (seed_override.rs line 410)
   pub fn require_seed_context_enabled() -> bool {
       cfg!(not(debug_assertions)) || env::var("AOS_REQUIRE_SEED_CONTEXT").is_ok()
   }

   // NEW
   pub fn require_seed_context_enabled() -> bool {
       env::var("AOS_ALLOW_SEED_FALLBACK").is_err()  // Opt-OUT instead of opt-IN
   }
   ```

2. Add explicit `AOS_ALLOW_SEED_FALLBACK=1` to test configurations only

3. Emit `StrictModeFailure` event when fallback would be used:
   ```rust
   pub fn derive_seed_contextual(label: &str) -> Result<[u8; 32]> {
       if get_thread_seed_context().is_none() {
           emit_strict_mode_failure_event("seed_context_missing", label);
           if !seed_fallback_allowed() {
               return Err(AosError::DeterminismViolation(
                   "Seed context required but not set. Set AOS_ALLOW_SEED_FALLBACK=1 for dev/test."
               ));
           }
           increment_seed_fallback_used();  // Metric for monitoring
           // ... fallback logic ...
       }
   }
   ```

4. Add preflight check in `aosctl preflight`:
   ```rust
   // Check that AOS_ALLOW_SEED_FALLBACK is NOT set in production
   if env::var("AOS_ALLOW_SEED_FALLBACK").is_ok() {
       warnings.push("AOS_ALLOW_SEED_FALLBACK is set - seed determinism not enforced");
   }
   ```

**Acceptance Criteria:**
- [ ] Missing seed context fails by default (no silent fallback)
- [ ] `seed_fallback_used_total` metric is 0 in production
- [ ] Tests explicitly opt-in to fallback via env var
- [ ] `aosctl preflight` warns if fallback enabled
- [ ] CI tests pass with explicit `AOS_ALLOW_SEED_FALLBACK=1`

**Tests to Add:**
- `tests/seed_context_required_test.rs` - Verify error without context
- Update all existing tests to set `AOS_ALLOW_SEED_FALLBACK=1`
- Integration test: verify preflight warning

---

### PR 4: Add V4 Receipt Schema with Complete Determinism Fields

**Scope:** Extend receipt schema to include all determinism-critical fields

**Code Targets:**
- `crates/adapteros-db/src/inference_trace.rs`
- `crates/adapteros-cli/src/commands/verify_receipt.rs`
- `crates/adapteros-api-types/src/receipt.rs` (if exists)

**Changes:**
1. Define V4 schema fields:
   ```rust
   // V4 additions (on top of V3)
   pub struct ReceiptDigestsV4 {
       // ... all V3 fields ...

       // Stop controller
       pub stop_reason_code: Option<String>,
       pub stop_reason_token_index: Option<u32>,

       // KV residency
       pub kv_residency_policy_id: Option<String>,
       pub kv_quota_enforced: Option<bool>,

       // Model/kernel binding
       pub model_cache_identity_v2_digest_b3_hex: Option<String>,
       pub kernel_plan_hash_b3_hex: Option<String>,
       pub metallib_hash_b3_hex: Option<String>,  // macOS only
   }
   ```

2. Update `compute_receipt_digest()` for V4:
   ```rust
   4 => {
       // V3 fields first
       buf.extend_from_slice(&compute_v3_prefix(...));

       // V4 additions (canonical order)
       buf.push(0x04);  // V4 marker
       append_optional_string(&mut buf, &stop_reason_code);
       append_optional_u32(&mut buf, stop_reason_token_index);
       append_optional_string(&mut buf, &kv_residency_policy_id);
       append_optional_bool(&mut buf, kv_quota_enforced);
       append_optional_b3(&mut buf, &model_cache_identity_v2_digest_b3_hex);
       append_optional_b3(&mut buf, &kernel_plan_hash_b3_hex);
       append_optional_b3(&mut buf, &metallib_hash_b3_hex);

       B3Hash::hash(&buf)
   }
   ```

3. Add migration for new receipt fields (if stored separately)

**Acceptance Criteria:**
- [ ] V4 receipts include all determinism-critical fields
- [ ] V1-V3 receipts still verify correctly
- [ ] V4 digest computation is deterministic
- [ ] Documentation lists all V4 fields with rationale

**Tests to Add:**
- `tests/receipt_v4_schema_test.rs`
- Backward compatibility: V1/V2/V3 fixtures still verify
- Determinism: same inputs produce identical V4 digests

---

### PR 5: Telemetry Bundle Chain Verification

**Scope:** Add periodic verification of telemetry bundle chain integrity

**Code Targets:**
- `crates/adapteros-telemetry/src/bundle_store.rs`
- `crates/adapteros-telemetry-verifier/src/lib.rs`
- `crates/adapteros-server/src/background_tasks/mod.rs`

**Changes:**
1. Add `verify_bundle_chain()` to bundle store:
   ```rust
   pub async fn verify_bundle_chain(
       &self,
       lookback_hours: u64,
   ) -> Result<BundleChainVerificationResult> {
       let bundles = self.list_bundles_since(lookback_hours).await?;
       let mut prev_merkle_root: Option<B3Hash> = None;

       for bundle in bundles.iter() {
           // Verify signature
           let valid_sig = self.verify_bundle_signature(&bundle)?;
           if !valid_sig {
               return Ok(BundleChainVerificationResult::SignatureInvalid { bundle_id: bundle.id });
           }

           // Verify chain linkage
           if let Some(expected_prev) = prev_merkle_root {
               if bundle.prev_bundle_hash != Some(expected_prev) {
                   emit_audit_export_tamper_event(&bundle);
                   return Ok(BundleChainVerificationResult::ChainBroken {
                       bundle_id: bundle.id,
                       expected: expected_prev,
                       actual: bundle.prev_bundle_hash,
                   });
               }
           }

           prev_merkle_root = Some(bundle.merkle_root);
       }

       Ok(BundleChainVerificationResult::Valid { bundles_verified: bundles.len() })
   }
   ```

2. Integrate with background task from PR 2

**Acceptance Criteria:**
- [ ] Bundle chain verified on configured interval
- [ ] `AuditExportTamper` event emitted on chain break
- [ ] Signature verification included
- [ ] Compression-transparent verification

**Tests to Add:**
- `tests/bundle_chain_verification_test.rs`
- Test: break chain, verify detection
- Test: invalid signature detected

---

### PR 6: Receipt Signing Location Audit and Hardening

**Scope:** Ensure receipt signing happens in control plane, not worker

**Code Targets:**
- `crates/adapteros-db/src/inference_trace.rs` (lines 452-582)
- `crates/adapteros-server-api/src/handlers/replies.rs`
- `crates/adapteros-crypto/src/signature.rs`

**Changes:**
1. Audit current signing location - verify signing key is NOT in worker process

2. If signing happens in worker, move to control plane:
   ```rust
   // In replies.rs handler (control plane)
   pub async fn serve_receipt(
       State(state): State<AppState>,
       Path(trace_id): Path<String>,
   ) -> Result<Json<ReceiptBundle>> {
       let mut receipt = state.db.get_receipt(&trace_id).await?;

       // Sign in control plane if not already signed
       if receipt.signature_b64.is_none() {
           let digest = B3Hash::from_hex(&receipt.receipt_digest_hex)?;
           let signature = state.signing_key.sign(digest.as_bytes());
           receipt.signature_b64 = Some(base64::encode(signature.to_bytes()));
           receipt.public_key_hex = Some(state.signing_key.public_key().to_hex());

           // Persist signature
           state.db.update_receipt_signature(&trace_id, &receipt).await?;
       }

       Ok(Json(receipt))
   }
   ```

3. Add key rotation audit trail:
   ```rust
   pub struct SigningKeyRotationEvent {
       old_key_id: String,
       new_key_id: String,
       rotated_at: DateTime<Utc>,
       reason: String,
   }
   ```

**Acceptance Criteria:**
- [ ] Signing key is NOT accessible to worker processes
- [ ] Receipts are signed in control plane on first access
- [ ] Key rotation is logged to audit trail
- [ ] Old signatures remain verifiable with archived public keys

**Tests to Add:**
- `tests/receipt_signing_location_test.rs`
- Verify worker cannot access signing key
- Key rotation audit trail test

---

## Risks / Unknowns

### 1. Receipt Digest Computation Divergence (HIGH RISK)
**Status:** Confirmed
**Description:** `verify_receipt.rs` and `inference_trace.rs` compute different digests due to missing fields.
**Impact:** All offline receipt verification fails for production receipts.
**Mitigation:** PR 1 (highest priority)

### 2. No Scheduled Audit Chain Verification (MEDIUM RISK)
**Status:** Confirmed
**Description:** Policy audit chain is only verified on-demand. Tampering could go undetected for extended periods.
**Impact:** Delayed detection of database tampering.
**Mitigation:** PR 2

### 3. Dev/Test Seed Fallback in Production (MEDIUM RISK)
**Status:** Potential
**Description:** If `AOS_REQUIRE_SEED_CONTEXT` is not set, fallback seeds may silently enter production.
**Impact:** Non-deterministic inference, replay failures.
**Mitigation:** PR 3

### 4. Receipt Signing Location Unknown (MEDIUM RISK)
**Status:** Needs investigation
**Description:** Unclear if signing happens in worker or control plane. If worker, compromised worker could forge receipts.
**Impact:** Receipt authenticity guarantees weakened.
**Mitigation:** PR 6

### 5. Telemetry Bundle Chain Not Verified (LOW RISK)
**Status:** Confirmed
**Description:** Bundle `prev_bundle_hash` chain is written but not periodically verified.
**Impact:** Tampering with exported bundles may go undetected.
**Mitigation:** PR 5

### 6. Tokenizer Hash Not Auto-Verified on Inference (LOW RISK)
**Status:** Confirmed
**Description:** Tokenizer hash is stored but not automatically verified when inference starts.
**Impact:** Modified tokenizer could produce different token IDs, affecting determinism.
**Mitigation:** Add tokenizer hash verification to inference startup (future PR)

### 7. Metallib Hash Enforcement Stubbed (LOW RISK)
**Status:** Noted in summary report
**Description:** Metal shader library hash enforcement may be partially implemented.
**Impact:** Different Metal shader versions could produce different results.
**Mitigation:** Include `metallib_hash_b3_hex` in V4 receipt schema (PR 4)

### 8. Ed25519 Key Management (MITIGATED)
**Status:** Needs audit
**Description:** Signing key lifecycle details are operationally sensitive.
**Impact:** Key compromise would allow receipt forgery.
**Mitigation:** Receipts now record signing key ID and signing timestamp (`receipt_signing_kid`, `receipt_signed_at`) to enable rotation audit and incident scoping. A full operational key management audit remains out of scope.

---

## Appendix: Reason Codes Reference

| Code | Description | Severity |
|------|-------------|----------|
| `CONTEXT_MISMATCH` | Input context digest mismatch | Error |
| `TRACE_TAMPER` | Token decision chain tampered | Error |
| `OUTPUT_MISMATCH` | Output tokens modified | Error |
| `POLICY_MISMATCH` | Policy mask digest mismatch | Error |
| `BACKEND_MISMATCH` | Backend expectations not met | Warning |
| `SIGNATURE_INVALID` | Ed25519 signature verification failed | Error |
| `BACKEND_ATTESTATION_MISMATCH` | Backend attestation hash mismatch | Error |
| `SCHEMA_VERSION_UNSUPPORTED` | Unknown receipt schema version | Error |
| `SEED_DIGEST_MISMATCH` | Seed substitution detected | Error |
| `SEED_MODE_VIOLATION` | Seed mode constraint violated | Error |
| `SEED_DIGEST_MISSING` | Seed digest required but absent | Error |
| `AUDIT_CHAIN_DIVERGED` | Policy audit chain broken | Critical |
| `AUDIT_EXPORT_TAMPER` | Telemetry bundle tampered | Critical |

---

## Appendix: File References

| File | Purpose |
|------|---------|
| `crates/adapteros-cli/src/commands/verify_receipt.rs` | CLI receipt verification |
| `crates/adapteros-db/src/inference_trace.rs` | Production receipt generation |
| `crates/adapteros-db/src/policy_audit.rs` | Policy audit chain |
| `crates/adapteros-core/src/seed.rs` | Seed derivation |
| `crates/adapteros-core/src/seed_override.rs` | Seed context management |
| `crates/adapteros-core/src/telemetry.rs` | Observability events |
| `crates/adapteros-telemetry/src/merkle.rs` | Merkle tree implementation |
| `crates/adapteros-telemetry/src/bundle.rs` | Bundle writer |
| `crates/adapteros-aos/src/writer.rs` | .AOS file writer |
| `crates/adapteros-aos/src/implementation.rs` | .AOS file reader |
