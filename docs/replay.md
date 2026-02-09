# Deterministic Replay API

**Status**: Production
**Version**: 3.0 (Consolidated Session-Based API)

---

## Overview

Replay enables verifying that inference operations are deterministic and reproducible. Given the same inputs and frozen configuration, replay produces identical outputs—proving that results are auditable and tamper-evident.

**Key distinction**: Replay proves **determinism** (same inputs = same outputs), not **correctness** (outputs match ground truth). Correctness requires a separate verdict system.

---

## Endpoint Families

There are three replay-related endpoint families. The session-based API is canonical.

| Family | Base Path | Status | Purpose |
|--------|-----------|--------|---------|
| Session-based | `/v1/replay/sessions` | **Canonical** | Full session management, verification, execution |
| Inference-based | `/v1/replay` | Deprecated | Legacy inference-level replay |
| Receipt verification | `/v1/adapteros/replay` | Active | Standalone receipt digest verification |

---

## Canonical Endpoint Family: Session-Based

The session-based API under `/v1/replay/sessions` is the canonical entry point for deterministic replay operations.

### List Sessions

```
GET /v1/replay/sessions
```

Lists all replay sessions, optionally filtered by tenant.

**Query Parameters**:
- `tenant_id` (optional): Filter sessions by tenant

**Response**: Array of `ReplaySessionResponse`

### Get Session

```
GET /v1/replay/sessions/{id}
```

Retrieves details of a specific replay session.

### Create Session

```
POST /v1/replay/sessions
```

Creates a replay session capturing the frozen configuration surface at a point in time.

**Request Body**:
```json
{
  "tenant_id": "tenant_abc",
  "cpid": "cpid_xyz",
  "plan_id": "plan_123",
  "telemetry_bundle_ids": ["bundle_1", "bundle_2"],
  "snapshot_at": "2024-01-15T10:30:00Z"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `tenant_id` | string | Yes | Tenant identifier |
| `cpid` | string | Yes | Control plane ID |
| `plan_id` | string | Yes | Plan identifier |
| `telemetry_bundle_ids` | array | Yes | Telemetry bundles to include |
| `snapshot_at` | string | No | ISO 8601 timestamp (defaults to now) |

**Response**: `ReplaySessionResponse` with generated session ID, hashes, and signature.

### Verify Session

```
POST /v1/replay/sessions/{id}/verify
```

Verifies the cryptographic integrity of a replay session including signature validation, hash chain integrity, and manifest/policy verification.

**Response**:
```json
{
  "session_id": "replay_abc123",
  "signature_valid": true,
  "hash_chain_valid": true,
  "manifest_verified": true,
  "policy_verified": true,
  "kernel_verified": true,
  "telemetry_verified": true,
  "overall_valid": true,
  "divergences": [],
  "verified_at": "2024-01-15T10:35:00Z"
}
```

**Note**: Kernel verification uses **fail-closed** semantics. Optional fields treated as invalid if absent—this prevents bypass vectors where missing fields are assumed valid.

### Execute Session

```
POST /v1/replay/sessions/{id}/execute
```

Re-runs inference under the frozen configuration and compares output to the original.

**Request Body**:
```json
{
  "use_original_rag_docs": true,
  "prompt": "Optional prompt override",
  "max_tokens": 100
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `use_original_rag_docs` | bool | false | Reconstruct RAG context from original documents |
| `prompt` | string | null | Override prompt (uses session prompt if not provided) |
| `max_tokens` | int | 100 | Maximum tokens to generate |

**Response**:
```json
{
  "session_id": "replay_abc123",
  "output": "Generated output text...",
  "degraded": false,
  "missing_doc_ids": [],
  "latency_ms": 150,
  "verified_at": "2024-01-15T10:36:00Z"
}
```

If `use_original_rag_docs` is true but documents are missing, the replay runs in **degraded mode** with `degraded: true` and `missing_doc_ids` populated.

---

## Receipt Verification Endpoints

### Verify Trace Receipt

```
POST /v1/replay/verify/trace
```

Verifies a receipt by recomputing digests from stored inference trace data.

**Request Body**:
```json
{
  "trace_id": "trace_abc123"
}
```

**Response**: `ReceiptVerificationResult` with digest comparisons and reason codes.

### Verify Bundle Receipt

```
POST /v1/replay/verify/bundle
```

Verifies a receipt bundle uploaded as multipart form data. Accepts JSON or ZIP archive containing evidence bundle.

**Form Field**: `bundle` - The evidence bundle file

**Supported Bundle Filenames** (for ZIP):
- `receipt_bundle.json`
- `run_receipt.json`
- `inference_trace.json`

### Standalone Receipt Verification

```
POST /v1/adapteros/replay
```

Verifies a receipt by digest lookup or inline payload. Does not create a session.

**Request Body**:
```json
{
  "receipt_digest": "abc123...",
  "payload": null
}
```

Provide either `receipt_digest` (hex-encoded BLAKE3) or `payload` (inline receipt bundle JSON).

### Get Receipt by Digest

```
GET /v1/adapteros/receipts/{digest}
```

Retrieves a stored receipt by its BLAKE3 digest (hex-encoded).

---

## Frozen Configuration Surface

For deterministic replay, the following must match between original and replay:

| Component | Storage Location | Required |
|-----------|------------------|----------|
| Base model ID + content hash | `manifest_hash_b3` | Yes |
| Adapter ID + content hash | `adapter_ids_json`, session state | Yes (or "none") |
| Router params | Session state | Yes |
| Sampling params (temperature, top_p, top_k, seed) | `sampling_params_json` | Yes |
| Backend ID | `backend` | Yes |
| Policy mask digest | `policy_hash_b3` | If policies applied |
| Build ID / commit hash | Receipt attestation | For full audit |
| RAG document hashes | `rag_snapshot_hash` | If RAG used |
| Dataset version ID | `dataset_version_id` | If dataset pinned |

### Router Parameters

The router uses deterministic tie-breaking: score DESC, then stable_id ASC. The `router_seed` is stored for audit purposes but does not affect routing decisions.

---

## Match Semantics

| Status | Definition |
|--------|------------|
| **Exact** | Token-for-token identical output AND receipt digest matches |
| **Semantic** | >80% word overlap but not token-identical (heuristic) |
| **Divergent** | Any difference in output or receipt |
| **Error** | Replay execution failed |

**Note**: Semantic matching is available but not recommended for determinism verification. Use **Exact** for audit-grade verification.

---

## Receipt Verification Reason Codes

When verification fails, reason codes indicate the cause:

| Code | Description |
|------|-------------|
| `CONTEXT_MISMATCH` | Context digest doesn't match recomputed value |
| `TRACE_TAMPER` | Run head hash or receipt digest mismatch |
| `OUTPUT_MISMATCH` | Output digest doesn't match |
| `POLICY_MISMATCH` | Policy mask digest differs from stored |
| `BACKEND_MISMATCH` | Backend or kernel version differs |
| `SIGNATURE_INVALID` | Cryptographic signature verification failed |
| `MISSING_RECEIPT` | No stored receipt found for trace |
| `TRACE_NOT_FOUND` | Trace ID doesn't exist |

---

## Receipt Schema Versions

Receipt digests use BLAKE3 with schema-versioned field encoding:

| Version | Added Fields |
|---------|--------------|
| V1 | Core fields (context, run_head, output, billing) |
| V2 | Backend identity (backend_used, backend_attestation) |
| V3 | Seed lineage (root_seed_digest, seed_mode, has_manifest_binding) |
| V4 | Stop controller, KV quota/residency, prefix cache, model cache |
| V5 | Equipment profile, citations (Patent 3535886.0002 Claims 6, 9-10) |
| V6 | Cross-run lineage (Patent 3535886.0002 Claims 7-8) |
| V7 | Determinism envelope, cache/tooling binding (current) |

Current production uses **V7** (`RECEIPT_SCHEMA_CURRENT`).

---

## Deprecated Endpoints

The following endpoints are deprecated and internally redirect to the canonical session-based family:

| Deprecated Endpoint | Canonical Alternative |
|---------------------|----------------------|
| `GET /v1/replay/check/{inference_id}` | Create session, then verify |
| `POST /v1/replay` | `POST /v1/replay/sessions/{id}/execute` |
| `GET /v1/replay/history/{inference_id}` | List sessions with tenant filter |

**Migration path**: Convert inference-based replay to session-based:
1. Create a session capturing the inference's frozen config
2. Use session verification/execution endpoints
3. Benefit: session can be re-verified/re-executed multiple times

---

## Invariants

1. **Sessions are immutable** - Once created, session configuration cannot be modified
2. **Sessions can be re-verified** - A session can be verified/executed multiple times
3. **Each execution is recorded** - Replay executions are logged to `replay_executions` table
4. **Receipt digests use BLAKE3** - All digests use BLAKE3 with schema version prefix
5. **Manifest match is required** - Replay fails if no worker has matching manifest hash
6. **Policy hooks apply** - Replay goes through same policy gates as normal inference (OnRequestBeforeRouting, OnBeforeInference, OnAfterInference)
7. **Tenant isolation enforced** - Sessions/traces are tenant-scoped; cross-tenant access denied

---

## Database Tables

### `replay_sessions`

Primary storage for replay session state.

| Column | Type | Description |
|--------|------|-------------|
| `id` | TEXT PK | Session ID (e.g., `replay_abc123`) |
| `tenant_id` | TEXT | Tenant isolation |
| `cpid` | TEXT | Control plane ID |
| `plan_id` | TEXT | Plan identifier |
| `snapshot_at` | TEXT | ISO 8601 timestamp |
| `seed_global_b3` | TEXT | Global seed hash (BLAKE3) |
| `rng_state_json` | TEXT | RNG state for deterministic replay |
| `manifest_hash_b3` | TEXT | Model manifest hash |
| `policy_hash_b3` | TEXT | Policy pack hash |
| `kernel_hash_b3` | TEXT | Kernel hash (optional) |
| `telemetry_bundle_ids_json` | TEXT | Array of bundle IDs |
| `adapter_state_json` | TEXT | Adapter snapshot |
| `routing_decisions_json` | TEXT | Routing decisions at snapshot |
| `inference_traces_json` | TEXT | Inference traces (optional) |
| `rag_state_json` | TEXT | RAG state for reconstruction |
| `signature` | TEXT | Ed25519 signature (hex) |

### `inference_replay_metadata`

Legacy storage for inference-level replay keys.

| Column | Type | Description |
|--------|------|-------------|
| `id` | TEXT PK | Metadata record ID |
| `inference_id` | TEXT UNIQUE | Links to inference |
| `tenant_id` | TEXT | Tenant isolation |
| `manifest_hash` | TEXT | Model manifest hash |
| `router_seed` | TEXT | Router seed (audit trail) |
| `sampling_params_json` | TEXT | Sampling configuration |
| `backend` | TEXT | Execution backend |
| `adapter_ids_json` | TEXT | Adapter IDs used |
| `prompt_text` | TEXT | Stored prompt (64KB limit) |
| `response_text` | TEXT | Stored response (64KB limit) |
| `rag_doc_ids_json` | TEXT | RAG document references |

### `replay_executions`

Audit trail of replay attempts.

| Column | Type | Description |
|--------|------|-------------|
| `id` | TEXT PK | Execution ID |
| `original_inference_id` | TEXT FK | Links to metadata |
| `replay_mode` | TEXT | exact/approximate/degraded |
| `match_status` | TEXT | exact/semantic/divergent/error |
| `executed_at` | TEXT | ISO 8601 timestamp |
| `executed_by` | TEXT | User who executed |

### `inference_trace_receipts`

Storage for cryptographic receipts.

| Column | Type | Description |
|--------|------|-------------|
| `trace_id` | TEXT PK | Links to inference trace |
| `run_head_hash` | BLOB | Running hash of decisions |
| `output_digest` | BLOB | Output token digest |
| `receipt_digest` | BLOB | Final receipt digest |
| `signature` | BLOB | Ed25519 signature (optional) |
| `schema_version` | INT | Receipt schema version (1-7) |

---

## Permissions

| Endpoint | Required Permission |
|----------|---------------------|
| `/v1/replay/sessions/*` | `ReplayManage` |
| `/v1/replay/verify/*` | `ReplayManage` |
| `/v1/replay/check/*` | `InferenceExecute` |
| `/v1/replay` | `InferenceExecute` |
| `/v1/adapteros/replay` | `ReplayManage` |
| `/v1/adapteros/receipts/*` | `InferenceExecute` |

---

## Testing

```bash
# Run replay determinism tests
cargo test -p adapteros-server-api --test replay_determinism_tests

# Verify golden replay (same inputs = same receipt)
cargo test -p adapteros-server-api golden_replay_produces_identical_receipt

# Run receipt verification tests
cargo test -p adapteros-server-api verify_bundle
```

---

## Related Documentation

- [Crypto Receipts](CRYPTO_RECEIPTS.md) - Receipt structure and verification
- [Determinism](DETERMINISM.md) - Determinism guarantees and seed handling
- [Token Caching Economics](TOKEN_CACHING_ECONOMICS.md) - Cache attribution in receipts
