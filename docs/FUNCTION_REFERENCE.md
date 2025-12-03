# AdapterOS Function Reference

**Purpose:** Verified ground-truth reference for key functions and components
**Last Verified:** 2025-12-02

This document provides accurate function names, locations, and line numbers verified against the actual codebase. Use this to avoid hallucinating function names.

---

## Inference & Routing

| Function/Struct | Location | Line | Notes |
|-----------------|----------|------|-------|
| `InferenceCore` | `server-api/src/inference_core.rs` | 56 | Main inference orchestration |
| `route_and_infer()` | `server-api/src/inference_core.rs` | 83 | Primary entry point |
| `route_and_infer_replay()` | `server-api/src/inference_core.rs` | 354 | Replay wrapper |
| `Router` struct | `lora-router/src/lib.rs` | 194 | K-sparse router |
| `route_with_adapter_info()` | `lora-router/src/lib.rs` | 943 | Primary routing method |
| `Decision` struct | `lora-router/src/lib.rs` | 1385 | indices, gates_q15, entropy |

## RAG Functions

| Function | Location | Line | Notes |
|----------|----------|------|-------|
| `retrieve_rag_context()` | `server-api/src/handlers/rag_common.rs` | 127 | Returns RagContextResult |
| `store_rag_evidence()` | `server-api/src/handlers/rag_common.rs` | 249 | Stores doc IDs, scores |
| `reconstruct_rag_context()` | `server-api/src/handlers/rag_common.rs` | 346 | For replay |

## Replay

| Function | Location | Line | Notes |
|----------|----------|------|-------|
| `check_availability()` | `server-api/src/handlers/replay_inference.rs` | 112 | GET /v1/replay/check/:id |
| `execute_replay()` | `server-api/src/handlers/replay_inference.rs` | 326 | POST /v1/replay |
| `get_replay_history()` | `server-api/src/handlers/replay_inference.rs` | 669 | GET /v1/replay/history/:id |

**Important:** Replay goes through `InferenceCore::route_and_infer_replay()`, NOT a bypass.

## Chat Context

| Function | Location | Line | Notes |
|----------|----------|------|-------|
| `build_chat_prompt()` | `server-api/src/chat_context.rs` | 77 | Multi-turn with token budget |

**Important:** Chat is multi-turn, NOT stateless. Full session history with truncation.

## Seed Derivation

| Function | Location | Line | Signature |
|----------|----------|------|-----------|
| `derive_seed()` | `adapteros-core/src/seed.rs` | 39 | `(global: &B3Hash, label: &str) -> [u8; 32]` |
| `derive_seed_typed()` | `adapteros-core/src/seed.rs` | 60 | Full entropy isolation |
| `derive_seed_indexed()` | `adapteros-core/src/seed.rs` | 80 | With index for batching |

## Handlers

Handlers use **direct names**, NOT `handle_*` prefix:

| Function | Location | Line | Endpoint |
|----------|----------|------|----------|
| `infer()` | `handlers/inference.rs` | 38 | POST /v1/infer |
| `batch_infer()` | `handlers/batch.rs` | 46 | POST /v1/batch |
| `streaming_infer()` | `handlers/streaming_infer.rs` | 460 | POST /v1/stream |

## Services

| Service | Location | Key Methods |
|---------|----------|-------------|
| `TrainingService` trait | `services/training_service.rs:38` | validate_training_request, check_training_capacity |
| `DefaultTrainingService` | `services/training_service.rs:98` | Implements trait |
| `AdapterService` trait | `services/adapter_service.rs:40` | promote_lifecycle, demote_lifecycle |
| `DefaultAdapterService` | `services/adapter_service.rs:133` | State machine impl |
| `Registry` | `adapteros-registry/src/lib.rs:17` | register_adapter:53 |
| `LifecycleManager` | `lora-lifecycle/src/lib.rs:163` | promote_adapter:1062, activate_stack:1862 |

## Backends

| Backend | Location | Constructor | Load Method |
|---------|----------|-------------|-------------|
| `MetalKernels` | `lora-kernel-mtl/src/lib.rs:192` | `new():255` | `load():1349` |
| `CoreMLBackend` | `lora-kernel-coreml/src/lib.rs:875` | `new():967` | `FusedKernels::load()` |
| `MLXFFIBackend` | `lora-mlx-ffi/src/backend.rs:42` | via module | `FusedKernels::load()` |

**Common trait:** `FusedKernels` at `lora-kernel-api/src/lib.rs:272`

**Factory:** `create_backend_with_model()` at `lora-worker/src/backend_factory.rs:328`
**Selection priority:** CoreML+ANE > Metal > MLX

## Policy & Audit

| Function | Location | Line | Notes |
|----------|----------|------|-------|
| `log_policy_decision()` | `adapteros-db/src/policy_audit.rs` | 111 | Merkle-chained append |
| `verify_policy_audit_chain()` | `adapteros-db/src/policy_audit.rs` | 226 | Chain verification |
| `GlobalTickLedger` | `deterministic-exec/src/global_ledger.rs` | 85 | NOT GlobalLedger |

## Cache

| Struct | Location | Line | Notes |
|--------|----------|------|-------|
| `ModelCache<K, T>` | `memory/src/model_cache.rs` | 98 | Generic LRU |
| `ModelCacheMetrics` | `memory/src/model_cache.rs` | 392 | hit_ratio, evictions |

## Telemetry

Actual telemetry functions (NOT `record_inference_event`):

| Function | Location | Notes |
|----------|----------|-------|
| `log_event()` | `telemetry/src/lib.rs` | General event logging |
| `record_metal_kernel_execution()` | `telemetry/src/metrics/critical_components.rs` | Metal tracking |
| `record_hotswap_latency()` | `telemetry/src/metrics/critical_components.rs` | Hotswap metrics |
| `record_adapter_state_transition()` | `telemetry/src/metrics/critical_components.rs` | Lifecycle |
| `record_determinism_violation()` | `telemetry/src/metrics/critical_components.rs` | Determinism |
| `InferenceEvent` struct | `telemetry/src/events/telemetry_events.rs` | Inference telemetry |

---

## What Does NOT Exist

| Speculated Name | Reality |
|-----------------|---------|
| `handle_infer()` | Use `infer()` |
| `handle_streaming_infer()` | Use `streaming_infer()` |
| `StackService` | Use `LifecycleManager::activate_stack()` |
| `TenantService` | Tenant ops in Db layer |
| `BackendCoordinator` | Use factory pattern |
| `BackendCapabilities` | Use `BackendHealth` / `PerformanceMetrics` |
| `GlobalLedger` | Actual: `GlobalTickLedger` |
| `ModelLoader::load_qwen_model_cached` | Does not exist |
| `record_inference_event()` | Use `log_event()` or `InferenceEvent` |

---

## Key Precision Values

| Value | Location | Notes |
|-------|----------|-------|
| Q15 encode | `lora-router/src/lib.rs:1022` | `(g * 32767.0).round() as i16` |
| Q15 decode | `lora-router/src/lib.rs:1397` | `q as f32 / 32767.0` |

**Denominator is 32767.0, NOT 32768.0**

---

## See Also

- [CLAUDE.md](../CLAUDE.md) - Developer guide with invariants
- [AGENTS.md](../AGENTS.md) - Agent-specific guide
- [ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - Architectural patterns
