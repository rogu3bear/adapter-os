# Execution Contract

This document specifies the determinism guarantees, canonicalization rules, and verification contracts for adapterOS inference. It serves as the single source of truth for auditors, integrators, and legal review.

**Version**: 1.0
**Schema**: Receipt V5 (Patent 3535886.0002 compliant)

---

## 1. Receipt Schema Versioning Policy

### Version Bump Criteria

| Change Type | Action | Example |
|-------------|--------|---------|
| Structural addition (new subsection) | Bump version | V4→V5 added equipment profile |
| Required field added | Bump version | V3 added seed lineage binding |
| Optional field added | No bump | Add `#[serde(default)]` field |
| Hash algorithm change | Bump version | Never done (BLAKE3 locked) |
| Field removed or redefined | Bump version | Avoid; prefer deprecation |

### Schema Timeline

| Version | Status | Era | Key Additions |
|---------|--------|-----|---------------|
| V1 | Legacy (read-only) | 2023 | Core: context, run_head, output, billing |
| V2 | Legacy (read-only) | 2023 | Backend identity binding |
| V3 | Legacy (read-only) | 2024 | Seed lineage (HKDF-SHA256) |
| V4 | Production | 2024 | Stop controller, KV quota, prefix cache, model cache |
| V5 | Current | 2026 | Equipment profile, citation binding (Patent 3535886.0002) |

**Support window**: 18-24 months after supersession.

### Dual Receipt Systems

| System | Location | Current Version | Purpose |
|--------|----------|-----------------|---------|
| ReceiptDigest | `receipt_digest.rs` | V5 | Production billing, audit trail |
| CryptoReceipt | `crypto_receipt.rs` | V2 | Third-party verification, scientific reproducibility |

CryptoReceipt is a cryptographic subset of ReceiptDigest. Contract C (V2) adds cached routing decisions to receipts.

**Code reference**: `crates/adapteros-core/src/receipt_digest.rs:25-34`

---

## 2. Canonicalization Rules

### Endianness

**Rule**: All numeric digest inputs use **little-endian** byte ordering.

```rust
// Canonical pattern throughout codebase
value.to_le_bytes()      // Encoding
u32::from_le_bytes(...)  // Decoding
```

**Code reference**: `crates/adapteros-core/src/receipt_digest.rs:284-285`

### String Encoding

| Context | Format | Example |
|---------|--------|---------|
| Adapter IDs | Length-prefixed UTF-8 | `[count:u32 LE][len:u32 LE][bytes]...` |
| JSON fields | Raw UTF-8 bytes | `policy_overrides_json.as_bytes()` |
| Empty strings | Length 0 | `""` → `[0u32 LE]` |

**Code reference**: `crates/adapteros-core/src/receipt_digest.rs:663-671` (`encode_adapter_ids`)

### Float Handling

**Rule**: Floating-point values are **excluded** from canonical hashing.

- Router gates use Q15 quantized integers (i16)
- Entropy fields are recorded but not hashed
- EOS probability thresholds use Q15 (denominator 32767.0)

**Rationale**: Float arithmetic is non-deterministic across platforms/compilers.

### Sentinel Values

| Type | None/Missing Value | Example |
|------|-------------------|---------|
| String | Empty string `""` | `stop_reason_code.unwrap_or("")` |
| u32 | `0xFFFFFFFF` | `stop_token_index.unwrap_or(0xFFFFFFFF)` |
| B3Hash | 32 zero bytes | `unwrap_or_else(\|\| vec![0u8; 32])` |
| bool | 0 (false) or 1 (true) | `if kv_quota_enforced { 1u8 } else { 0u8 }` |

**Code reference**: `crates/adapteros-core/src/receipt_digest.rs:16-20`

### Token Serialization

```
Format: [token_count: u32 LE] [token_0: u32 LE] ... [token_n: u32 LE]
```

Each token is exactly 4 bytes (u32). Length prefix prevents count ambiguity.

**Code reference**: `crates/adapteros-core/src/receipt_digest.rs:556-604`

---

## 3. EquipmentProfile Spec

### Field Definitions

| Field | Type | Required | Fallback | Example |
|-------|------|----------|----------|---------|
| `processor_id` | String | Yes | `"unknown"` | `"Apple M4 Max"` |
| `engine_version` | String | Yes | `"unknown"` | `"mlx-0.21.0"` |
| `ane_version` | Option<String> | No | None | `"ANEv4-38core"` |
| `digest` | B3Hash | Auto | Computed | BLAKE3 of fields |

### Detection Priority

1. **Processor ID**: `sysctl machdep.cpu.brand_string` (macOS) → `"unknown"`
2. **Engine Version**: `MLX_VERSION` env → compile-time constant → `"unknown"`
3. **ANE Version**: Chip detection (M4→ANEv4, M3→ANEv3, M2→ANEv2, M1→ANEv1) → None

### Digest Computation

```rust
// BLAKE3 of concatenated fields
hasher.update(processor_id.as_bytes());
hasher.update(engine_version.as_bytes());
hasher.update(ane_version.unwrap_or("none").as_bytes());
```

**Code reference**: `crates/adapteros-core/src/crypto_receipt.rs:114-154`

---

## 4. Hydration State Machine

### Runtime Loading States

```
Unloaded → Cold → Warm → Hot → Resident
```

| State | Definition | Eviction Priority |
|-------|------------|-------------------|
| Unloaded | Not in memory | N/A |
| Cold | Weights loaded, not in rotation | Low |
| Warm | In rotation pool, occasionally selected | Medium |
| Hot | Frequently selected, prioritized | High |
| Resident | Always active (pinned) | Never |

**Code reference**: `crates/adapteros-lora-lifecycle/src/state.rs:73-80`

### Adapter Lifecycle States

```
Draft → Training → Ready → Active → Deprecated → Retired
                ↘            ↘              ↗
                 └────→ Failed (ephemeral: Active → Retired)
```

| State | Determinism Support | Notes |
|-------|---------------------|-------|
| Draft | No | Work in progress |
| Training | No | Weights unstable |
| Ready | **Yes** | Artifact validated |
| Active | **Yes** | In production |
| Deprecated | No | Pending retirement |
| Retired | No | Terminal |
| Failed | No | Terminal |

**Invariant**: Only `Ready` and `Active` states guarantee deterministic inference.

**Code reference**: `crates/adapteros-core/src/lifecycle.rs:141-142`

### Loading Sequence

1. Load adapter weights (from .sealed, .aos, or .safetensors)
2. Verify hash against expected (BLAKE3)
3. Load tokenizer (AOS_TOKENIZER_PATH or model directory)
4. Initialize prefix KV cache (if configured)
5. Compile Metal kernels (if not cached)
6. Initialize router state

---

## 5. Backend Cache Invalidation

### Cache Key Computation

| Cache Type | Key Components | Hash Algorithm |
|------------|----------------|----------------|
| Metal kernels | Compiled metallib binary | BLAKE3 |
| MLX headers | Header file content | BLAKE3 |
| Model cache | model_id + adapter_hashes + config | BLAKE3 (`model_cache_identity_v2`) |
| Prefix KV | context_digest + tokenizer_hash | BLAKE3 |

**Code reference**: `crates/adapteros-lora-kernel-mtl/build.rs:342-351`

### GC Policy

| Parameter | Default | Description |
|-----------|---------|-------------|
| TTL | 1 hour | Entries expire after idle timeout |
| Eviction | LRU | Least-recently-used evicted first |
| Max entries | Configurable | Oldest deleted when exceeded |
| Coherence | Generation-based | Stack generation change resets cache |

**Code reference**: `crates/adapteros-lora-mlx-ffi/src/adapter_cache.rs:87-103`

### Corruption Detection

- **Build-time**: Manifest signature verification (Ed25519)
- **Build-time**: Kernel hash comparison against manifest
- **Runtime**: Generation counter invalidates stale cache entries

**Invariant**: Cache must not change outputs. Same inputs with cache hit must produce identical results as cache miss.

---

## 6. Adapter Sealing Chain

### Cryptographic Primitives

| Component | Algorithm | Size |
|-----------|-----------|------|
| Content hash | BLAKE3 | 32 bytes |
| Signature | Ed25519 | 64 bytes |
| Container magic | "SEAL" | 4 bytes |

### Sealing Format

```
Header (144 bytes, aligned):
├─ Magic: "SEAL" (4 bytes)
├─ Version: 1 (4 bytes)
├─ Integrity hash: BLAKE3(version || manifest || payload) (32 bytes)
├─ Payload offset/size (16 bytes)
├─ Manifest offset/size (16 bytes)
├─ Ed25519 signature (64 bytes)
└─ Reserved (8 bytes)
```

**Code reference**: `crates/adapteros-aos/src/sealed.rs`

### Key Rotation Policy

| Mode | Trigger | Default Interval |
|------|---------|------------------|
| Scheduled | Automatic timer | 90 days |
| Manual | Admin command | On demand |
| Emergency | Suspected compromise | Immediate |

**Code reference**: `crates/adapteros-crypto/src/rotation_daemon.rs`

### Tenant Binding

- `AdapterScope` enum: Global, Tenant, Repo, Commit
- Scope checked at load time via `AdapterIntegrityVerifier`
- Manifest includes scope declaration

**Code reference**: `crates/adapteros-lora-worker/src/adapter_integrity.rs:137`

---

## 7. Routing Digest + Caching (Contract C)

### Per-Token Decision Hash

```rust
// hash_token_decision() components
├─ context_digest: [u8; 32]
├─ token_index: u32
├─ adapter_ids_blob: length-prefixed encoding
├─ gates_blob: Q15 quantized gates (i16 array)
├─ policy_mask_digest: Optional[u8; 32]
├─ allowed_mask_blob: Optional bool array
├─ policy_overrides_json: raw bytes
├─ backend_id: String
└─ kernel_version_id: String
```

**Code reference**: `crates/adapteros-core/src/receipt_digest.rs:610-647`

### Run-Head Chain

```rust
run_head[i] = BLAKE3(prev_run_head || decision_hash[i] || token_index[i])
```

**Code reference**: `crates/adapteros-core/src/receipt_digest.rs:652-658`

### Contract C: Cached Routing

When prefix cache hit occurs:
1. Load routing decisions from `PrefixKvEntry::routing_decisions`
2. Convert to `CommittedDecision` format
3. Prepend to generated decisions
4. Compute `run_head_hash` over full chain (cached + generated)

**Invariant**: Cache hit must produce identical `run_head_hash` as if routing was computed fresh.

**Code reference**: `crates/adapteros-lora-worker/src/prefix_kv_cache.rs:32-92`

---

## 8. Stop Controller Determinism

### Stop Reason Codes

| Code | Priority | Trigger |
|------|----------|---------|
| `BUDGET_MAX` | 1 (highest) | Hard token limit exceeded |
| `COMPLETION_CONFIDENT` | 2 | EOS probability > Q15 threshold |
| `REPETITION_GUARD` | 3 | N-gram repetition detected |
| `STOP_SEQUENCE` | 4 | Explicit stop sequence matched |
| `LENGTH` | 5 (lowest) | EOS token encountered |

**Invariant**: Priority order is deterministic. Same token sequence always stops at same reason/index.

### Q15 Threshold Quantization

```rust
let eos_prob_q15 = (eos_prob * 32767.0).round() as i16;
```

Denominator `32767.0` matches router Q15 pattern.

**Code reference**: `crates/adapteros-lora-worker/src/stop_controller.rs`

### Cross-Backend Guarantee

Stop controller runs **post-inference** in Rust, independent of backend. Logits computed by backend are fed to deterministic stop logic.

---

## 9. Ready Endpoint Truth Table

### /healthz (Liveness)

| Boot State | HTTP Status | Response |
|------------|-------------|----------|
| Failed | 503 | `"failed: [code] message"` |
| Booting (any) | 503 | `"booting: {state}"` |
| Ready | 200 | `"healthy"` |
| FullyReady | 200 | `"healthy"` |
| Degraded | 200 | `"degraded"` |
| Maintenance | 200 | `"maintenance"` |
| Draining/Stopping | 200 | `"draining: {state}"` |

### /readyz (Readiness)

| Mode | DB Check | Worker Check | Models Check | Result |
|------|----------|--------------|--------------|--------|
| Strict | Required | Required | Required | All must pass |
| Relaxed | Required | Skippable | Skippable | DB + configured checks |
| DevBypass | Ignored | Ignored | Ignored | Always 200 |

### Check Cascade

If DB check fails, worker and models checks are **skipped** (not failed). This is intentional: downstream checks depend on DB connectivity.

**Code reference**: `crates/adapteros-server-api/src/handlers/health.rs:168-528`

### UI Contract

- UI hydration waits for `/readyz = 200`
- UI shows loading state when `/readyz = 503`
- `Degraded` state: Server ready, UI may show warning banner

---

## 10. Replay Parity Contract

### Identical Conditions Definition

| Dimension | Enforcement | Verification |
|-----------|-------------|--------------|
| Model hash | Required | `ModelAvailabilityChecker` |
| Adapter hashes | Required | `AdapterAvailabilityChecker` |
| Backend tier | Required | `BackendKind::determinism_tier()` |
| Request seed | Required | Stored in `ReproducibleReplaySpec` |
| Router seed | Required | Stored as hex string |
| HKDF version | Required | Const guard (version 2) |

### Seed Derivation

```rust
// HKDF-SHA256 with BLAKE3 global seed as IKM
HKDF_ALGORITHM_VERSION = 2
HKDF_OUTPUT_LENGTH = 32 bytes

// Domain separation via labels
"router", "sampling", "adapter_0", ...
```

**Code reference**: `crates/adapteros-core/src/seed.rs:1021-1072`

### Guarantee Levels

| Level | Backend | Seeds | Guarantee |
|-------|---------|-------|-----------|
| `exact` | MLX + Strict | Manifest-bound | Bitwise identical |
| `approximate` | CoreML/Metal + BestEffort | Tenant-scoped | Functionally equivalent |
| `none` | Fallback | Relaxed | No guarantee |

---

## 11. Air-Gapped Security Posture

### Network Binding

| Mode | Binding | Notes |
|------|---------|-------|
| Development | TCP (localhost) | `--insecure-skip-egress-check` |
| Production | UDS only | No TCP/UDP binding |

### Egress Policy

| Rule | Enforcement |
|------|-------------|
| No outbound TCP/UDP | PF rules + socket validation |
| No DNS in serving | Policy layer blocks |
| UDS-only IPC | Hard-coded in production mode |
| Verified media import | Signature + SBOM required |

### PF Validation (macOS)

```bash
# Required rules
sudo pfctl -e                    # Enable PF
echo 'block out all' | sudo pfctl -f -  # Deny all egress
```

**Code reference**: `crates/adapteros-policy/src/egress.rs`

### Security Preflight Checklist

- [ ] PID file lock (single-writer guarantee)
- [ ] PF enabled with deny-all rules
- [ ] No dev bypass env vars (`AOS_DEV_NO_AUTH`, etc.)
- [ ] JWT secret not placeholder
- [ ] Fingerprint baseline matches (drift detection)

---

## 12. Future Work

The following items are acknowledged gaps, not yet implemented:

| Item | Priority | Notes |
|------|----------|-------|
| Linux PF equivalent | Medium | macOS only; needs iptables/netfilter |

**Completed in v0.12.2**:
- Compile cache key enforcement (backend_compile_flags_hash in manifests/metadata)

**Completed in v0.13.1**:
- Key revocation mechanism (RevokedKey rejection in SealedAdapterLoader)
- Cross-backend stop tests (simulated backend parity tests)

---

## Appendix A: Critical Test Matrix

| # | Test | Status | Location | Proves |
|---|------|--------|----------|--------|
| 1 | Receipt round-trip verify | **EXISTS** | `determinism_core_suite.rs:136-246` | Digest chain stable |
| 2 | Deterministic tie-break | **EXISTS** | `router_stability.rs:12-41` | Same scores → same pick |
| 3 | Q15 gate encode/decode | **EXISTS** | `determinism_core_suite.rs:50-133` | 32767 denom invariant |
| 4 | Backend selection policy | **EXISTS** | `determinism_hardening_tests.rs:318-370` | Strict→no best-effort |
| 5 | Cached-span routing digest | **EXISTS** | `prefix_kv_cache_integration.rs:1343-1483` | Cache reuse parity (Contract C) |
| 6 | Stop reason determinism | **EXISTS** | `stop_controller_inference_integration.rs:1001-1169` | Cross-backend parity |
| 7 | Hydration gating | **EXISTS** | `tests/hydration_gating_test.rs:1-222` | UI Ready → Server Ready |
| 8 | Adapter seal verify | **EXISTS** | `sealed_adapter_receipt_binding.rs:316-349` | Revoked key blocks load |
| 9 | ContextId stability | **EXISTS** | `prefix_kv_cache_integration.rs:441-516` | Same inputs → same id |
| 10 | Replay parity | **EXISTS** | `replay_identical.rs:80-202` | run A == replay(A) |

**Coverage**: 10/10 fully exist

---

## References

- `crates/adapteros-core/src/receipt_digest.rs` - Receipt schema and digest computation
- `crates/adapteros-core/src/seed.rs` - HKDF seed derivation
- `crates/adapteros-core/src/crypto_receipt.rs` - CryptoReceipt and EquipmentProfile
- `crates/adapteros-lora-worker/src/stop_controller.rs` - Stop reason logic
- `crates/adapteros-server-api/src/handlers/health.rs` - Ready endpoints
- `crates/adapteros-policy/src/egress.rs` - Air-gapped security
- `crates/adapteros-aos/src/sealed.rs` - Adapter sealing
