# MoE-LoRA Rename Map

## Overview

This document lists identifiers that need to be renamed to comply with the [NAMING_MOE_LORA.md](./NAMING_MOE_LORA.md) contract.

**Status:** Pending review and approval before renaming PRs merge.

---

## Summary

| Category | Identifiers | Definitions | Total Usages |
|----------|-------------|-------------|--------------|
| Lora → LoRA | 11 | 13 | ~50+ files |
| Moe → MoE | 9 | 9 | ~5 files |
| Rope → RoPE | 2 | 4 | ~5 files |
| Kv → KV (cache) | 1 | 1 | 11 files |
| Mlx → MLX | 1 enum variant | 1 | 1 file |
| Q15 bugs | - | 4-5 | 4-5 files |

**Total: 24 identifiers across 70+ file locations**

---

## Lora → LoRA Renames (11 identifiers)

### `LoraConfig` → `LoRAConfig`

**Definitions (2):**
| Location | Line |
|----------|------|
| `crates/adapteros-lora-kernel-mtl/src/fused_qkv.rs` | 164 |
| `crates/adapteros-lora-kernel-mtl/src/fused_mlp.rs` | 20 |

**Usages (5):**
- `crates/adapteros-lora-kernel-mtl/src/lib.rs` (re-export)
- `crates/adapteros-lora-kernel-mtl/tests/metal_inference_pipeline_tests.rs`
- `tests/kernel_dropout_bias_tests.rs`
- `tests/kernel_tests.rs`
- `src/lib.rs` (commented)

### `LoraTier` → `LoRATier`

**Definition:**
| Location | Line |
|----------|------|
| `crates/adapteros-types/src/training/mod.rs` | 109 |

**Usages (13+ files):**
- `crates/adapteros-server-api/src/handlers.rs`
- `crates/adapteros-server-api/src/handlers/adapter_lifecycle.rs`
- `crates/adapteros-server-api/src/handlers/adapters_read.rs`
- `crates/adapteros-server-api/src/handlers/adapter_utils.rs`
- `crates/adapteros-orchestrator/src/training.rs`
- `crates/adapteros-lora-worker/src/training/packager.rs`
- `crates/adapteros-lora-lifecycle/src/loader.rs`
- `crates/adapteros-api-types/src/training.rs`
- `crates/adapteros-api-types/src/adapters.rs`
- `crates/adapteros-types/src/adapters/metadata.rs`
- `xtask/src/main.rs`
- `xtask/src/pack_lora.rs`

### `PackLoraArgs` → `PackLoRAArgs`

**Definition:**
| Location | Line |
|----------|------|
| `xtask/src/pack_lora.rs` | 8 |

**Usages:** Same file + `xtask/src/main.rs`

### `MockLoraWeights` → `MockLoRAWeights`

**Definition:**
| Location | Line |
|----------|------|
| `crates/adapteros-lora-kernel-mtl/tests/metal_inference_pipeline_tests.rs` | 77 |

**Usages:** Same file only (test helper)

### `LoraFusionConfig` → `LoRAFusionConfig`

**Definition:**
| Location | Line |
|----------|------|
| `crates/adapteros-lora-kernel-coreml/src/fusion.rs` | 48 |

**Usages:**
- `crates/adapteros-lora-kernel-coreml/src/lib.rs`
- `crates/adapteros-lora-kernel-coreml/src/moe.rs`

### `LoraTarget` → `LoRATarget`

**Definition:**
| Location | Line |
|----------|------|
| `crates/adapteros-lora-kernel-coreml/src/fusion.rs` | 74 |

**Usages:**
- `crates/adapteros-lora-kernel-coreml/src/lib.rs`
- `crates/adapteros-lora-kernel-coreml/src/moe.rs`

### `ParsedLoraWeights` → `ParsedLoRAWeights`

**Definition:**
| Location | Line |
|----------|------|
| `crates/adapteros-lora-kernel-coreml/src/fusion.rs` | 131 |

**Usages:**
- `crates/adapteros-lora-kernel-coreml/src/lib.rs`
- `crates/adapteros-lora-kernel-coreml/src/moe.rs`

### `TelemetryLoraWeights` → `TelemetryLoRAWeights`

**Definition:**
| Location | Line |
|----------|------|
| `crates/adapteros-lora-worker/src/telemetry_lora.rs` | 30 |

**Usages:**
- `crates/adapteros-lora-worker/src/lib.rs`
- `crates/adapteros-lora-worker/src/telemetry_adapter.rs`

### `TelemetryLoraRegistry` → `TelemetryLoRARegistry`

**Definition:**
| Location | Line |
|----------|------|
| `crates/adapteros-lora-worker/src/telemetry_lora.rs` | 94 |

**Usages:**
- `crates/adapteros-lora-worker/src/lib.rs`
- `crates/adapteros-lora-worker/src/telemetry_adapter.rs`

### `VisionLoraWeights` → `VisionLoRAWeights`

**Definition:**
| Location | Line |
|----------|------|
| `crates/adapteros-lora-worker/src/vision_lora.rs` | 37 |

**Usages:**
- `crates/adapteros-lora-worker/src/lib.rs`

### `VisionLoraRegistry` → `VisionLoRARegistry`

**Definition:**
| Location | Line |
|----------|------|
| `crates/adapteros-lora-worker/src/vision_lora.rs` | 134 |

**Usages:**
- `crates/adapteros-lora-worker/src/lib.rs`

---

## Moe → MoE Renames (9 identifiers)

All in `crates/adapteros-lora-kernel-coreml/src/moe.rs`:

| Current | Target | Line |
|---------|--------|------|
| `MoeConfig` | `MoEConfig` | 21 |
| `MoeLoraStrategy` | `MoELoRAStrategy` | 62 |
| `MoeLoraTarget` | `MoELoRATarget` | 91 |
| `MoeLoraWeights` | `MoELoRAWeights` | 110 |
| `MoeAdapterWeights` | `MoEAdapterWeights` | 174 |
| `MoeGpuFingerprint` | `MoEGpuFingerprint` | 213 |

All in `crates/adapteros-lora-mlx-ffi/src/moe.rs`:

| Current | Target | Line |
|---------|--------|------|
| `QuantizedMoeConfig` | `QuantizedMoEConfig` | 37 |
| `MlxRsMoeLayer` | `MlxRsMoELayer` | 86 |
| `QuantizedMoeLayer` | `QuantizedMoELayer` | 354 |

**Additional usages:**
- `crates/adapteros-lora-kernel-coreml/src/lib.rs` (re-exports)

---

## Rope → RoPE Renames (2 identifiers, 4 definitions)

### `RopeScaling` → `RoPEScaling`

**Definitions (3 - duplicated struct):**
| Location | Line |
|----------|------|
| `crates/adapteros-manifest/src/lib.rs` | 49 |
| `crates/adapteros-lora-plan/src/config.rs` | 54 |
| `crates/adapteros-lora-mlx-ffi/src/model.rs` | 42 |

**Usages:**
- `tests/policy_gates_qwen.rs`

### `RopeConfig` → `RoPEConfig`

**Definition:**
| Location | Line |
|----------|------|
| `crates/adapteros-lora-plan/src/loader.rs` | 413 |

**Usages:**
- `crates/adapteros-manifest/src/lib.rs`
- `crates/adapteros-lora-mlx-ffi/src/model.rs`

---

## KV Cache Renames (1 identifier, 11 files)

### `KvResidency` → `KVResidency`

**Definition:**
| Location | Line |
|----------|------|
| `crates/adapteros-lora-kernel-mtl/src/kv_cache.rs` | 31 |

**Usages (10 files):**
- `crates/adapteros-lora-kernel-mtl/src/lib.rs`
- `crates/adapteros-lora-kernel-mtl/src/gpu_memory_pool.rs`
- `crates/adapteros-server-api/src/handlers/replay.rs`
- `crates/adapteros-db/src/inference_trace.rs`
- `crates/adapteros-api-types/src/inference.rs`
- `crates/adapteros-telemetry/src/lib.rs`
- `crates/adapteros-telemetry/src/events/telemetry_events.rs`
- `tests/kv_residency_quota_integration.rs`
- `tests/kv_residency_quota_tests.rs`
- `tests/kv_quota_concurrent_e2e.rs`

**Note:** Other `Kv*` types (e.g., `UserKv`, `DocumentKv`) are key-value store entities, not cache acronyms. These use `Kv` as a word suffix intentionally and do NOT require renaming.

---

## MLX Enum Variant (1 location)

### `BackendHint::Mlx` → `BackendHint::MLX`

| Location | Line |
|----------|------|
| `crates/adapteros-lora-backends/src/lib.rs` | 55, 71, 89 |

---

## Q15 Denominator Bug Fixes

The Q15 maximum is 2^15 - 1 = **32767**. Using 32768.0 causes max gate = 0.99997 instead of 1.0.

### Actual Bugs (require fix)

| Location | Line | Current | Fix |
|----------|------|---------|-----|
| `crates/adapteros-lora-kernel-mtl/src/debug.rs` | 80 | `/ 32768.0` | `/ 32767.0` |
| `crates/adapteros-lora-kernel-mtl/src/coreml_backend.rs` | 578 | `/ 32768.0` | `/ 32767.0` |
| `crates/adapteros-testing/src/kernel_testing.rs` | 506 | `/ 32768.0` | `/ 32767.0` |
| `crates/adapteros-lora-router/tests/router_weights_config.rs` | 618 | `/ 32768.0` | `/ 32767.0` |

### Needs Investigation

| Location | Line | Issue |
|----------|------|-------|
| `crates/adapteros-lora-worker/src/training/quantizer.rs` | 11-12 | `LORA_Q15_MIN = -32768.0` and `LORA_Q15_DENOM = 32768.0` - verify if intentional for signed range |

### Legitimate Uses (no change)

These use `32768.0` correctly as clamp bounds or test assertions:
- `scripts/create_aos_adapters.rs:73` - clamp bound `(-32768.0, 32767.0)`
- `scripts/aos_packager/src/main.rs:82` - clamp bound
- `crates/adapteros-lora-router/src/lib.rs:37` - documentation
- `crates/adapteros-lora-router/tests/q15_edge_cases.rs` - tests for incorrect usage
- `crates/adapteros-server-api/tests/q15_conversion_test.rs` - tests for incorrect usage

---

## Already Correct (No Changes Needed)

### MoE Types (using MoE)
- `MoEConfigManifest` ✓
- `MoEPrefixEntry`, `MoEPrefixCache`, `MoEPrefixCacheStats` ✓
- `MoEInfo` ✓
- `MoETrainingConfig` ✓
- `MoELoRAStrategy` (in trainer.rs) ✓

### LoRA Types (using LoRA)
- `LoRAConfig` (in mlx-ffi, db) ✓
- `LoRAAdapter` ✓
- `LoRADelta`, `LoadedLoRAConfig` ✓
- `ANELoRAConfig` ✓
- `MicroLoRATrainer`, `SeparatedLoRATrainer` ✓
- `LoRAWeights`, `LoRAQuantizer`, `QuantizedLoRAWeights` ✓
- `LoRAMergeVisualization` ✓
- `LmHeadLoRA` ✓

### KV Cache Types (using KV)
- `KVCacheConfig` ✓
- `MLXKVCache` ✓
- `KVCache` ✓
- `KVCacheManager` ✓
- `LayerKVCache` ✓

### MLX Types (using MLX)
- `MLXFFIBackend`, `MLXFFIModel`, `MLXFFITensor` ✓
- `MLXKVCache` ✓
- `MLXMonitor`, `MLXMemoryPool`, `MLXQuantizer` ✓
- `MLXAdapterCache`, `MLXTokenizer` ✓
- `MLXStreamingGenerator`, `MLXGenerator` ✓
- `MLXEmbeddingModel`, `MLXMemoryManager` ✓
- `MLXResilienceConfig` ✓

### RoPE Types (using RoPE)
- `RoPECache` ✓
- `RoPEFrequencies` ✓

### Q15 Constants
- `ROUTER_GATE_Q15_DENOM = 32767.0` ✓ (in lora-router)
- `gates_q15` ✓

---

## Execution Plan

### Phase 1: High Impact (LoRA - affects most files)
1. `LoraTier` → `LoRATier` (14+ files)
2. `LoraConfig` → `LoRAConfig` in kernel-mtl (7 files)

### Phase 2: CoreML MoE (single file, many types)
3. All `Moe*` → `MoE*` in `kernel-coreml/moe.rs` (6 types)
4. All `*Lora*` → `*LoRA*` in `kernel-coreml/fusion.rs` (3 types)

### Phase 3: MLX MoE
5. All `*Moe*` → `*MoE*` in `lora-mlx-ffi/moe.rs` (3 types)

### Phase 4: Worker LoRA types
6. `TelemetryLora*` → `TelemetryLoRA*` (2 types, 4 files)
7. `VisionLora*` → `VisionLoRA*` (2 types, 2 files)

### Phase 5: Misc
8. `PackLoraArgs` → `PackLoRAArgs` (xtask)
9. `MockLoraWeights` → `MockLoRAWeights` (test)
10. `KvResidency` → `KVResidency` (11 files)
11. `RopeScaling` → `RoPEScaling` (4 definitions)
12. `RopeConfig` → `RoPEConfig` (3 files)
13. `BackendHint::Mlx` → `BackendHint::MLX` (1 file)

### Phase 6: Bug Fixes
14. Fix Q15 `32768.0` → `32767.0` (4 files)

---

## Verification Checklist

After all renames:

```bash
# Should return 0 results each:
grep -r "struct Lora" --include="*.rs" | grep -v "LoRA"
grep -r "enum Lora" --include="*.rs" | grep -v "LoRA"
grep -r "struct Moe" --include="*.rs" | grep -v "MoE"
grep -r "enum Moe" --include="*.rs" | grep -v "MoE"
grep -r "KvResidency" --include="*.rs"
grep -r "RopeScaling\|RopeConfig" --include="*.rs"
grep -r "BackendHint::Mlx" --include="*.rs"
```

```bash
# Build and test
cargo build
cargo test
```

---

## Out of Scope (Future Consideration)

### Mlx* → MLX* Types

The naming contract specifies `MLX*` for type names, but there are ~15 `Mlx*` types in the codebase:

```
MlxDevice, MlxBridgeConfig, MlxRsModelConfig, MlxRsModel,
MlxRsExpertWeights, MlxRsMoeLayer, MlxArrayGuard, MlxArrayVecGuard,
MlxSamplerConfig, MlxTokenAlternative, MlxTokenMetadata,
MlxDeviceType, MlxBackendCapabilities, MlxArray, etc.
```

These are inconsistent with existing `MLX*` types (`MLXFFIBackend`, `MLXKVCache`, etc.) but were not part of the original PRD scope focused on MoE/LoRA terminology.

**Recommendation:** Address in a separate PR after MoE/LoRA renames are complete.

---

## Changelog

| Date | Change |
|------|--------|
| 2025-12-25 | Initial rename map |
| 2025-12-25 | Complete rewrite with verified grep counts |
