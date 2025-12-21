# PRD-RECT-005: Model Loading — Integrity + Graceful Error Handling

## Problem / Motivation

A 20-agent investigation identified 6 critical issues in the model loading pipeline (`backend_factory.rs`, `model_handle_cache.rs`):

1. **MODEL_CACHE panic**: Missing `AOS_MODEL_CACHE_MEMORY_BUDGET` caused panic at startup
2. **No hash verification**: Model bytes loaded without integrity verification against registry
3. **Pinned entry memory leak**: No mechanism to detect or clean up stale pinned entries
4. **Sharded model truncation**: Missing shards silently ignored, causing truncated model loads
5. **Silent config.json failures**: Parse errors swallowed via `.ok()?`, hiding misconfigurations
6. **GQA config validation**: Invalid `num_key_value_heads > num_attention_heads` not caught

## Goals

- Make model loading **fail-fast** on integrity/config issues rather than silently degrading
- Provide **graceful error handling** instead of panics for missing configuration
- Add **observability** for pinned entry leaks
- Enable **hash verification** infrastructure for control plane integration

## Non-Goals

- Full control plane integration to pass `weights_hash` to workers (future work)
- UI exposure of model cache internals

## Requirements

### R1. Graceful MODEL_CACHE initialization

`MODEL_CACHE` must not panic if memory budget env var is missing. Instead, return a Result that callers can handle.

### R2. Hash verification support

Provide `load_model_bytes_verified(path, expected_hash)` that:
- Computes BLAKE3 hash of loaded bytes
- Returns error on mismatch when `expected_hash` is provided
- Logs computed hash for audit when no expected hash provided

### R3. Pinned entry leak detection + cleanup

Add methods to `ModelHandleCache`:
- `pinned_keys()` — list all pinned model keys
- `pinned_memory_bytes()` — total memory held by pinned entries
- `stale_pinned_entries(threshold)` — entries pinned longer than threshold
- `unpin_all()` — force-unpin all entries
- `cleanup_all()` — evict everything (for shutdown)

Integrate `cleanup_all()` into worker shutdown path.

### R4. Sharded model completeness validation

`detect_sharded_model()` must:
- Parse shard filenames via regex (`model-{N}-of-{M}.safetensors`)
- Validate all shards 1..M exist
- Return error listing missing shard indices if incomplete

### R5. Visible config.json parse failures

Replace `.ok()?` with explicit match that logs errors at `error!` level before returning `None`.

### R6. GQA config validation

`ModelConfig::validate()` must check `num_key_value_heads <= num_attention_heads` and return error if violated.

## Acceptance Criteria

- `get_model_cache()` returns `Result<&'static ModelHandleCache>` — no panics
- `load_model_bytes_verified()` with mismatched hash returns `AosError::Config`
- `cleanup_all()` called during worker drain (verified in `aos_worker.rs`)
- Missing shard returns error with missing indices listed
- Invalid GQA config returns validation error
- All 30 `model_handle_cache` tests pass
- All 11 `backend_factory` tests pass

## Implementation Summary

| Issue | Fix | Location |
|-------|-----|----------|
| MODEL_CACHE panic | `Lazy<Result<ModelHandleCache, String>>` + `get_model_cache()` helper | `backend_factory.rs:89-102` |
| Hash verification | `load_model_bytes_verified(path, expected_hash)` | `backend_factory.rs:1874` |
| Pinned entry leak | 5 new methods + shutdown integration | `model_handle_cache.rs`, `aos_worker.rs:1534` |
| Sharded truncation | `detect_sharded_model()` with regex validation | `backend_factory.rs:2134` |
| Silent config failures | Explicit match with `error!()` logging | `backend_factory.rs:169` |
| GQA validation | `ModelConfig::validate()` in `load_and_validate_model_config()` | `backend_factory.rs:169` |

## Test Plan

```bash
cargo test -p adapteros-lora-worker --lib -- model_handle_cache  # 30 tests
cargo test -p adapteros-lora-worker --lib -- backend_factory     # 11 tests
```

New tests added:
- `test_pinned_keys_returns_all_pinned`
- `test_pinned_memory_bytes`
- `test_stale_pinned_entries_detection`
- `test_unpin_all`
- `test_cleanup_all_evicts_everything`
- `test_verify_model_integrity_mismatch`
- `test_compute_model_directory_hash_single_file`

## Rollout / Risk

- **Low risk**: Changes are additive error handling, not behavioral changes to happy path
- **Shutdown cleanup**: May slightly increase shutdown latency if many models pinned
- **Future integration**: Control plane can pass `weights_hash` to enable full verification
