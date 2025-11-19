# Batch Inference: Critical Fix Required

**Status:** Broken in Production
**Severity:** P0 (Blocker)
**Fix Time:** <5 minutes

---

## The Problem

Frontend and backend disagree on the batch inference endpoint URL:

```
Backend: POST /v1/infer/batch   ✅ Working
Frontend: POST /api/batch/infer ❌ Returns 404
```

Result: Frontend calls fail with HTTP 404. Batch inference is broken.

---

## The Fix (Choose One)

### Option A: Fix Frontend (Recommended)

**File:** `/Users/star/Dev/aos/ui/src/api/client.ts`
**Line:** 920

Change from:
```typescript
return this.request<types.BatchInferResponse>('/api/batch/infer', {
```

To:
```typescript
return this.request<types.BatchInferResponse>('/v1/infer/batch', {
```

**Rationale:** Follows `/v1/` pattern (consistent with all other inference endpoints like `/v1/infer`)

### Option B: Fix Backend

**File:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/routes.rs`
**Line:** 424

Change from:
```rust
.route("/v1/infer/batch", post(handlers::batch::batch_infer))
```

To:
```rust
.route("/api/batch/infer", post(handlers::batch::batch_infer))
```

Also update the OpenAPI path in `/crates/adapteros-server-api/src/handlers/batch.rs` line 19:
```rust
path = "/api/batch/infer",
```

---

## Verification After Fix

```bash
# Rebuild backend (if chose Option B)
cargo build --release -p adapteros-server-api

# Test with curl
curl -X POST http://localhost:5000/v1/infer/batch \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "requests": [
      {"id": "test-1", "prompt": "hello"}
    ]
  }'

# Expected: 200 OK with batch response (not 404)
```

---

## Files Affected

| File | Change | Reason |
|------|--------|--------|
| `ui/src/api/client.ts:920` | Update URL | Fix frontend to use correct endpoint |
| `ui/src/components/inference/BatchInference.test.md:11` | Update docs | Reflect correct endpoint (informational) |

---

## Impact

After fix:
- ✅ Frontend batch requests succeed
- ✅ UI shows results correctly
- ✅ Error handling works
- ✅ Timeouts respected
- ✅ Partial failure support active

---

## Timeline

- **Fix:** <5 minutes (1-line change)
- **Test:** <5 minutes (single test request)
- **Merge:** Immediate (no dependencies)

