# Cryptographic Receipt Integration Audit

## Overview

This document audits the integration of the new `crypto_receipt` module (`crates/adapteros-core/src/crypto_receipt.rs`) with the existing adapterOS infrastructure for inference trace receipts.

## Component Mapping

### New Module vs Existing Infrastructure

| Component | `crypto_receipt` (New) | Existing Infrastructure |
|-----------|------------------------|-------------------------|
| Context binding | `ContextId` | `TraceStart.context_digest` |
| Input digest | `compute_input_digest()` | N/A (not computed separately) |
| Routing accumulation | `RoutingDigest` | `SqlTraceSink.run_head_hash` |
| Output digest | `compute_output_digest()` | `SqlTraceSink.output_digest()` |
| Equipment profile | `EquipmentProfile` | `DeviceFingerprint` + `ReceiptDigestInput.equipment_profile_digest_b3` |
| Final receipt | `CryptographicReceipt` | `TraceReceipt` + `RunReceipt` |
| Receipt builder | `ReceiptGenerator` | `SqlTraceSink` |

### Detailed Integration Points

#### 1. Trace Finalization (`crates/adapteros-lora-worker/src/lib.rs`)

**Current Flow:**
```
generate() → trace_sink.finalize(TraceFinalization) → TraceReceipt → RunReceipt
```

**Integration Opportunity:**
- The `ReceiptGenerator` can be used alongside or as a wrapper for `SqlTraceSink`
- `CryptographicReceipt` provides a cleaner API for the finalization step
- Equipment profile should be passed through `TraceFinalization`

**Action Items:**
1. Add `equipment_profile_digest_b3` field to `TraceFinalization`
2. Wire `DeviceFingerprint.compute_equipment_digest()` at worker startup
3. Consider exposing `CryptographicReceipt` in API responses

#### 2. Token Recording (`crates/adapteros-db/src/inference_trace.rs`)

**Current Flow:**
```
SqlTraceSink::record_token() → hash_decision() → update_head()
```

**Equivalent in `crypto_receipt`:**
```rust
generator.record_routing_decision(RoutingRecord {
    step,
    input_token_id,
    adapter_indices,
    gates_q15,
    entropy,
    policy_mask_digest,
});
```

**Compatibility:**
- `RoutingRecord.compute_hash()` uses a different serialization than `SqlTraceSink.hash_decision()`
- The chain accumulation formula is compatible: `BLAKE3(prev || step || decision_hash)`
- **Gap:** `crypto_receipt` doesn't include `backend_id` and `kernel_version_id` per token

**Action Items:**
1. Add `backend_id` and `kernel_version_id` fields to `RoutingRecord`
2. Ensure hash serialization matches existing `hash_decision()` for replay verification

#### 3. Receipt Digest Computation

**Current Implementation (`inference_trace.rs:322-394`):**
- Includes: context, run_head, output, billing, stop controller, KV quota, prefix cache, model cache identity
- Does NOT include: equipment profile (that's in V5 schema in `receipt_digest.rs`)

**`crypto_receipt` Implementation:**
- Includes: context_id, input_digest, routing_digest, output_digest, equipment_profile
- Does NOT include: billing fields, stop controller, KV quota, prefix cache, model cache identity

**Gap Analysis:**
| Field | `crypto_receipt` | `inference_trace` | `receipt_digest.rs` V5 |
|-------|------------------|-------------------|------------------------|
| context_id | ✅ | ✅ (context_digest) | ✅ |
| input_digest | ✅ | ❌ | ❌ |
| run_head_hash | ✅ (routing_digest) | ✅ | ✅ |
| output_digest | ✅ | ✅ | ✅ |
| equipment_profile | ✅ | ❌ | ✅ |
| billing fields | ❌ | ✅ | ✅ |
| stop controller | ❌ | ✅ | ✅ |
| KV quota | ❌ | ✅ | ✅ |
| prefix cache | ❌ | ✅ | ✅ |
| model cache identity | ❌ | ✅ | ✅ |
| citations | ❌ | ❌ | ✅ |

**Action Items:**
1. Either extend `CryptographicReceipt` with billing/stop/KV/cache fields
2. OR use `crypto_receipt` as a complementary verification layer
3. Document which receipt type to use for which purpose

#### 4. Equipment Profile Integration

**Current State:**
- `DeviceFingerprint` in `crates/adapteros-verify/src/metadata.rs` computes equipment digest
- `ReceiptDigestInput` in V5 schema has equipment fields
- `inference_trace.rs` does NOT populate equipment fields

**Integration Path:**
```rust
// At worker startup
let fingerprint = DeviceFingerprint::capture_current()?;
let equipment_digest = fingerprint.compute_equipment_digest();

// Store for use during inference
worker_state.equipment_profile = Some(EquipmentProfile {
    processor_id: fingerprint.processor_id.clone(),
    engine_version: fingerprint.mlx_version.clone(),
    ane_version: fingerprint.ane_version.clone(),
    digest: equipment_digest,
});

// During receipt generation
generator.set_equipment_profile(
    &fingerprint.processor_id.unwrap_or_default(),
    &fingerprint.mlx_version.unwrap_or_default(),
);
```

**Action Items:**
1. Capture `DeviceFingerprint` at worker initialization
2. Pass equipment profile through to receipt generation
3. Store equipment profile in `inference_trace_receipts` table (migration exists: `0285_equipment_profile.sql`)

#### 5. Tenant Binding

**Current State:**
- `TraceStart.tenant_id` captures tenant at trace creation
- Audit logs are tenant-scoped (`audit.rs`)
- Evidence envelopes include tenant binding
- Migration `0288_tenant_binding.sql` adds `tenant_binding_mac`

**`crypto_receipt` Support:**
- `ReceiptMetadata.tenant_id` field available
- `ReceiptGenerator.set_tenant_id()` method available

**Integration:**
```rust
// Set tenant at generator creation
generator.set_tenant_id(&request.tenant_id);

// After finalization
receipt.metadata.tenant_id == Some(trace_start.tenant_id)
```

#### 6. API Response Integration

**Current Flow:**
```
InferenceResponse.run_receipt (Option<RunReceipt>) → API JSON response
```

**Proposed Enhancement:**
```rust
// Option A: Add crypto_receipt alongside run_receipt
pub struct InferenceResponse {
    pub run_receipt: Option<RunReceipt>,
    pub crypto_receipt_digest: Option<String>, // Add for verification
}

// Option B: Embed in RunReceipt
pub struct RunReceipt {
    // existing fields...
    pub crypto_receipt_digest_b3: Option<String>,
    pub input_digest_b3: Option<String>,
}
```

## Integration Recommendations

### Phase 1: Minimal Integration (Recommended First)

1. **Use `crypto_receipt` for input digest computation**
   - Add `input_digest_b3` field to `TraceReceipt` and `RunReceipt`
   - Call `compute_input_digest()` in `SqlTraceSink::new()` or `finalize()`

2. **Wire equipment profile**
   - Capture `DeviceFingerprint` at worker startup
   - Pass to `TraceFinalization`
   - Store in receipt table

3. **Add verification endpoint**
   - New `/v1/receipts/:trace_id/verify` endpoint
   - Returns `CryptographicReceipt` with verification status

### Phase 2: Full Integration

1. **Replace `SqlTraceSink` routing with `ReceiptGenerator`**
   - Requires matching hash algorithms exactly
   - Provides cleaner API for new code

2. **Extend `CryptographicReceipt` with billing fields**
   - Add `ReceiptBillingInfo` struct
   - Include in final digest computation

3. **Add `CryptographicReceipt` to evidence envelope**
   - Create `InferenceReceiptRef` from `CryptographicReceipt`
   - Chain-link in tenant-scoped evidence chain

### Phase 3: Migration

1. **Compute `crypto_receipt` digests for existing receipts**
   - Background job to recompute
   - Store as additional column

2. **Dual-write during transition**
   - Write both `TraceReceipt` and `CryptographicReceipt`
   - Compare digests for consistency

## Code Changes Required

### `crates/adapteros-db/src/inference_trace.rs`

```rust
// Add to TraceFinalization
pub struct TraceFinalization<'a> {
    // ... existing fields ...
    pub equipment_profile_digest_b3: Option<B3Hash>,
    pub processor_id: Option<String>,
    pub mlx_version: Option<String>,
    pub ane_version: Option<String>,
}

// Add input digest tracking
impl SqlTraceSink {
    pub async fn new_with_input_tokens(
        db: Arc<Db>,
        start: TraceStart,
        input_tokens: &[u32],
        flush_every: usize,
    ) -> Result<Self> {
        let input_digest = adapteros_core::compute_input_digest_v2(input_tokens);
        // ... store input_digest for later use
    }
}
```

### `crates/adapteros-lora-worker/src/lib.rs`

```rust
// At worker initialization
let fingerprint = DeviceFingerprint::capture_current()?;
let equipment_profile = adapteros_core::EquipmentProfile::compute(
    fingerprint.processor_id.as_deref().unwrap_or("unknown"),
    fingerprint.mlx_version.as_deref().unwrap_or("unknown"),
    fingerprint.ane_version.as_deref(),
);

// Store in worker state
self.equipment_profile = Some(equipment_profile);

// During finalization
let finalization = TraceFinalization {
    // ... existing fields ...
    equipment_profile_digest_b3: Some(self.equipment_profile.as_ref().map(|p| p.digest)),
    processor_id: fingerprint.processor_id.clone(),
    mlx_version: fingerprint.mlx_version.clone(),
    ane_version: fingerprint.ane_version.clone(),
};
```

### Database Migration

```sql
-- Migration: Add input_digest to inference_trace_receipts
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS input_digest_b3 BYTEA;

-- Index for verification queries
CREATE INDEX IF NOT EXISTS idx_receipts_input_digest
    ON inference_trace_receipts(input_digest_b3)
    WHERE input_digest_b3 IS NOT NULL;
```

## Testing Strategy

1. **Unit Tests**: Already comprehensive in `crypto_receipt.rs` (19 tests)
2. **Integration Tests**: Add to `tests/` for end-to-end receipt generation
3. **Replay Tests**: Verify receipt digests match between implementations
4. **Determinism Tests**: Ensure same inputs produce same receipts across runs

## Open Questions

1. Should `CryptographicReceipt` replace or complement `TraceReceipt`?
2. Should billing fields be added to `CryptographicReceipt` or kept separate?
3. What's the migration strategy for existing receipts?
4. Should the routing decision hash algorithm be unified?

## Conclusion

The `crypto_receipt` module provides a clean, specification-compliant implementation of cryptographic receipt generation. Integration with the existing infrastructure requires:

1. **Immediate**: Wire equipment profile, add input digest
2. **Short-term**: Expose in API, add verification endpoint
3. **Long-term**: Consider as replacement for `SqlTraceSink` receipt logic

The module's design aligns well with existing patterns but introduces `input_digest` as a new binding that improves verifiability.
