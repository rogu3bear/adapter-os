# Determinism Patch Inventory

This document inventories the minimal patch series for deterministic routing fixes, build provenance
binding, and associated documentation updates.

## Invariants Enforced

- Router tie-breaking is canonical and deterministic: `score DESC, stable_id ASC`.
- Workers receive `adapter_stable_ids` when routing determinism is enabled, so tie-breaking is
  stable across adapter ordering/filtering.
- Receipts bind routing-affecting code constants (for example `PINNED_BOOST`) via build provenance
  (`model_build_hash_b3`).
- Receipt `context_digest` serialization is shared and canonicalized (worker + verifiers compute
  identical digests), with a golden test locking the output.
- DB migration policy is consistent with shipped migrations: legacy sequential migrations are
  capped at `0301` and `adapters.stable_id` semantics/backfill ordering are preserved.
- Only the explicitly intended deletions are present: `crates/adapteros-cli/src/commands/trace.rs`
  and `crates/adapteros-server-api/src/handlers/debug.rs`.

## Patch Series

### 1) `router: canonical stable_id tie-break`

Files changed:

- `crates/adapteros-lora-router/src/router.rs`
- `crates/adapteros-lora-router/tests/determinism.rs`
- `tests/router_stability.rs`
- `tests/cross_platform_determinism.rs`

Invariant enforced:

- Router selection is deterministic with stable tie-breaking using `stable_id` (not array index),
  and the hard-routing (`tau=0`) path selects the canonical top candidate.

### 2) `cp/worker: plumb adapter_stable_ids + bind build provenance`

Files changed:

- `crates/adapteros-server-api/src/inference_core/core.rs`
- `crates/adapteros-server-api/src/types/request.rs`
- `crates/adapteros-server-api/src/types/context.rs`
- `crates/adapteros-server-api/src/types/replay.rs`
- `crates/adapteros-server-api/src/uds_client.rs`
- `crates/adapteros-lora-worker/src/request_types.rs`
- `crates/adapteros-lora-worker/src/lib.rs`
- `crates/adapteros-lora-worker/src/inference_pipeline.rs`
- `crates/adapteros-lora-worker/src/cache_warmup.rs`
- `crates/adapteros-lora-worker/src/uds_server.rs`
- `crates/adapteros-core/src/third_party_verification.rs`

Invariant enforced:

- Stable IDs are resolved in the control plane and sent to workers for deterministic routing.
- The worker binds build provenance into receipts via `model_build_hash_b3`, anchoring routing
  semantics to a trusted build identity.

### 3) `cleanup: delete obsolete trace/debug entrypoints`

Files changed:

- `crates/adapteros-cli/src/commands/mod.rs`
- `crates/adapteros-cli/src/commands/trace.rs` (deleted)
- `crates/adapteros-server-api/src/handlers/debug.rs` (deleted)

Invariant enforced:

- Only canonical, maintained debug/trace surfaces remain; the removed entrypoints are not part of
  determinism guarantees and were explicitly approved for deletion.

### 4) `streaming: include adapter_stable_ids field`

Files changed:

- `crates/adapteros-api/src/streaming.rs`
- `crates/adapteros-server-api/src/http/streaming.rs`

Invariant enforced:

- All direct `InferenceRequest` builders remain schema-compatible with the worker request type
  after adding `adapter_stable_ids` (even when the caller cannot resolve stable IDs and passes
  `None`).

### 5) Docs Updates (tie-break + provenance)

Files changed:

- `docs/ARCHITECTURE.md`
- `docs/COREML_DETERMINISM_AUDIT_TRAILS.md`
- `docs/DEPLOYMENT.md`
- `docs/DETERMINISM.md`
- `docs/EXECUTION_CONTRACT.md`
- `docs/TECHNICAL_SPECIFICATION.md`
- `docs/TESTING.md`
- `docs/replay.md`
- `docs/runbooks/DETERMINISM_VIOLATION.md`

Invariant enforced:

- Documentation consistently reflects the canonical tie-break (`stable_id`), and clarifies that
  routing-affecting constants are anchored via build provenance.

### 6) `db: allow sequential migrations up to 0301`

Files changed:

- `crates/adapteros-db/tests/migration_conflicts.rs`
- `docs/DB_MIGRATIONS.md`

Invariant enforced:

- Migration-numbering policy matches shipped migrations (sequential series capped at `0301`);
  future migrations should use timestamps. The `adapters.stable_id` meaning and deterministic
  backfill ordering remain unchanged.

### 7) `core: canonicalize receipt context_digest serializer`

Files changed:

- `crates/adapteros-core/src/context_digest.rs`
- `crates/adapteros-core/src/lib.rs`
- `crates/adapteros-core/src/third_party_verification.rs`
- `crates/adapteros-crypto/src/receipt_verifier.rs`
- `crates/adapteros-lora-worker/src/lib.rs`
- `crates/adapteros-cli/tests/verify_receipt_tests.rs`

Invariant enforced:

- The exact `context_digest` byte layout is defined in a single canonical function, and reused by
  the worker and verifier paths. A golden test locks the digest output for fixed inputs.

## Verification

Commands to reproduce:

```bash
# Router determinism (crate-level)
cargo test -p adapteros-lora-router --test determinism

# Router determinism (repo-level integration tests)
cargo test --test router_stability

# Compile-check the updated control-plane/worker boundary
cargo check -p adapteros-server-api
cargo check -p adapteros-lora-worker

# Migration numbering policy
cargo test -p adapteros-db --test migration_conflicts

# Canonical context_digest serializer (producer/verifier parity)
cargo test -p adapteros-core golden_context_digest_worker_layout
cargo test -p adapteros-core third_party_verification
cargo test -p adapteros-crypto --test receipt_payload_vectors
cargo test -p adapteros-cli --test verify_receipt_tests
```

Tests run:

- `cargo test -p adapteros-lora-router --test determinism`
- `cargo test --test router_stability`
- `cargo check -p adapteros-server-api`
- `cargo check -p adapteros-lora-worker`
- `cargo test -p adapteros-db --test migration_conflicts`
- `cargo test -p adapteros-core golden_context_digest_worker_layout`
- `cargo test -p adapteros-core third_party_verification`
- `cargo test -p adapteros-crypto --test receipt_payload_vectors`
- `cargo test -p adapteros-cli --test verify_receipt_tests`
