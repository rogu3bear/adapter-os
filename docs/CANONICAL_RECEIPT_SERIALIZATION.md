# Canonical Receipt Serialization (Inference Receipt Digest V7)

This document defines a single, re-implementable receipt serialization and digest specification for **inference receipts** in AdapterOS.

Scope:
- Canonical payload type: `adapteros_core::receipt_digest::ReceiptDigestInput`
- Canonical digest entrypoint: `adapteros_core::receipt_digest::compute_receipt_digest(..., RECEIPT_SCHEMA_V7)`
- Canonical JSON serialization entrypoint: `adapteros_core::receipt_digest::canonical_json_string(...)`

Non-scope:
- Cancellation receipts (see `crates/adapteros-core/src/crypto_receipt.rs`)
- Any legacy/non-canonical receipt generator paths

## 1. Versioning And Domain Separation

The canonical inference receipt digest uses:
- Schema version marker byte: `0x07` (`RECEIPT_SCHEMA_V7`)
- Hash function: **BLAKE3-256** (32-byte output)

Domain separation is provided by the schema marker being the **first** byte hashed for V5+ (including V7). A V7 digest is therefore not interchangeable with V4/V6 digests even for identical field values.

## 2. Canonical JSON Encoding (Interchange Format)

The canonical, portable encoding for `ReceiptDigestInput` is **canonical JSON** as produced by:
`adapteros_core::receipt_digest::canonical_json_string`.

Rules:
1. UTF-8 encoding, no BOM.
2. JSON objects are serialized with keys sorted lexicographically (byte-wise UTF-8 ordering).
3. Arrays preserve order.
4. No extra whitespace (compact JSON).
5. Numbers are written in decimal form as produced by `serde_json`:
   - Integers use base-10 digits, no leading `+`, no leading zeros (except `0`), no exponent notation.
   - `NaN`/`Infinity` are not permitted (the receipt schema should not include them).
6. `null` is used for absent optional fields (`Option<T> = None`).

Test vectors for canonical JSON are stored under `docs/receipt_test_vectors/v7/`.

## 3. Digest Input Framing (Byte-Level)

The receipt digest is computed over a deterministic, byte-framed sequence of fields.

Primitive encodings:
- `u32`, `u64`, `i16` are little-endian (`to_le_bytes()`).
- `bool` is a single byte: `0x00` (false) or `0x01` (true).
- Fixed digests (`[u8; 32]`) are included as 32 raw bytes.
- Strings are UTF-8 bytes preceded by a `u32` little-endian length.

Optional field sentinels (MUST match current implementation):
- `Option<String>`: treat `None` as empty string (`len = 0`, `bytes = []`).
- `Option<[u8; 32]>`: treat `None` as 32 zero bytes.
- `Option<u32>`: treat `None` as `0xFFFFFFFF`.
- `Option<i16>`: treat `None` as `i16::MIN` (`0x8000`).

Important: this framing is the digest contract. Canonical JSON is the interchange format for the payload; the digest is not computed over JSON bytes.

## 4. ReceiptDigestInput Field Semantics

`ReceiptDigestInput` is a flattened digest payload. It contains:
- Core binding: `context_digest`, `run_head_hash`, `output_digest`
- Token accounting: `logical_*`, `prefix_cached_token_count`, `billed_*`
- Stop-controller binding, KV/cache binding, equipment/citation binding, lineage, and V7 determinism/tooling bindings.

See `crates/adapteros-core/src/receipt_digest.rs` for the authoritative schema.

## 5. V7 Field Order (Authoritative)

To compute `receipt_digest_v7`, hash the following in order (this list is normative and must match `compute_v7_digest`):

1. `[0x07]` schema version marker (single byte)
2. `context_digest` (32 bytes)
3. `run_head_hash` (32 bytes)
4. `output_digest` (32 bytes)
5. `logical_prompt_tokens` (u32 LE)
6. `prefix_cached_token_count` (u32 LE)
7. `billed_input_tokens` (u32 LE)
8. `logical_output_tokens` (u32 LE)
9. `billed_output_tokens` (u32 LE)
10. `backend_used` (u32 LE length + bytes; empty if None)
11. `backend_attestation_b3` (u32 LE length + bytes; `len=0` if None)
12. `stop_reason_code` (u32 LE length + bytes; empty if None)
13. `stop_reason_token_index` (u32 LE; `0xFFFFFFFF` if None)
14. `stop_policy_digest_b3` (32 bytes; all zeros if None)
15. `tenant_kv_quota_bytes` (u64 LE)
16. `tenant_kv_bytes_used` (u64 LE)
17. `kv_evictions` (u32 LE)
18. `kv_residency_policy_id` (u32 LE length + bytes; empty if None)
19. `kv_quota_enforced` (u8)
20. `prefix_kv_key_b3` (32 bytes; all zeros if None)
21. `prefix_cache_hit` (u8)
22. `prefix_kv_bytes` (u64 LE)
23. `model_cache_identity_v2_digest_b3` (32 bytes; all zeros if None)
24. `equipment_profile_digest_b3` (32 bytes; all zeros if None)
25. `processor_id` (u32 LE length + bytes; empty if None)
26. `mlx_version` (u32 LE length + bytes; empty if None)
27. `ane_version` (u32 LE length + bytes; empty if None)
28. `citations_merkle_root_b3` (32 bytes; all zeros if None)
29. `citation_count` (u32 LE)
30. `previous_receipt_digest` (32 bytes; all zeros if None)
31. `session_sequence` (u64 LE)
32. `tokenizer_hash_b3` (32 bytes; all zeros if None)
33. `tokenizer_version` (u32 LE length + bytes; empty if None)
34. `tokenizer_normalization` (u32 LE length + bytes; empty if None)
35. `model_build_hash_b3` (32 bytes; all zeros if None)
36. `adapter_build_hash_b3` (32 bytes; all zeros if None)
37. `decode_algo` (u32 LE length + bytes; empty if None)
38. `temperature_q15` (i16 LE; `i16::MIN` if None)
39. `top_p_q15` (i16 LE; `i16::MIN` if None)
40. `top_k` (u32 LE; `0xFFFFFFFF` if None)
41. `seed_digest_b3` (32 bytes; all zeros if None)
42. `sampling_backend` (u32 LE length + bytes; empty if None)
43. `thread_count` (u32 LE; `0xFFFFFFFF` if None)
44. `reduction_strategy` (u32 LE length + bytes; empty if None)
45. `stop_eos_q15` (i16 LE; `i16::MIN` if None)
46. `stop_window_digest_b3` (32 bytes; all zeros if None)
47. `cache_scope` (u32 LE length + bytes; empty if None)
48. `cached_prefix_digest_b3` (32 bytes; all zeros if None)
49. `cached_prefix_len` (u32 LE; `0xFFFFFFFF` if None)
50. `cache_key_b3` (32 bytes; all zeros if None)
51. `retrieval_merkle_root_b3` (32 bytes; all zeros if None)
52. `retrieval_order_digest_b3` (32 bytes; all zeros if None)
53. `tool_call_inputs_digest_b3` (32 bytes; all zeros if None)
54. `tool_call_outputs_digest_b3` (32 bytes; all zeros if None)
55. `disclosure_level` (u32 LE length + bytes; empty if None)

The digest is `BLAKE3_256(concat(all framed fields in order))`.

## 6. Golden Test Vectors

Vectors live at:
- `docs/receipt_test_vectors/v7/minimal.input.json`
- `docs/receipt_test_vectors/v7/typical.input.json`
- `docs/receipt_test_vectors/v7/citations_equipment.input.json`

Each has a corresponding expected digest:
- `*.expected_receipt_digest_hex.txt`

The AdapterOS test suite asserts:
- canonical JSON serialization is stable (`deserialize -> serialize` matches fixture)
- computed V7 digest matches the expected hex
- single-field mutation changes the digest

