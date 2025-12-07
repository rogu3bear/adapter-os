# AdapterOS Inference Flow

## Current Architecture (As of 2025-12-07)

### Canonical, code-matched behavior

- Entry points: HTTP `infer`, `batch_infer`, `streaming_infer`, and replay all flow through `InferenceCore::route_and_infer()`; replay uses `route_and_infer_replay()` but still hits the same routing guard.
- Base model gate: aggregated `ModelLoadStatus` (from `model_status.rs`) must be `ready` before routing; otherwise an `ApiErrorBody` is returned with HTTP 503 and `code="MODEL_NOT_READY"` plus the `request_id`.
- ModelLoadStatus literals (kebab-case): `no-model`, `loading`, `ready`, `unloading`, `error`, `checking`. Aggregation precedence: any ready → ready; else loading → loading; else checking → checking; else unloading → unloading; else error → error; else no-model.
- Metrics (Prometheus): `adapteros_model_load_success_total`, `adapteros_model_load_failure_total`, `adapteros_model_unload_success_total`, `adapteros_model_unload_failure_total`, and gauge `adapteros_model_loaded{model_id,tenant_id}` (1 = ready, 0 = not ready).
- Inference error codes (via `InferenceError`): `MODEL_NOT_READY`, `NO_COMPATIBLE_WORKER`, `BACKPRESSURE`, `PERMISSION_DENIED`, `RAG_ERROR`, `ROUTING_BYPASS`, `REQUEST_TIMEOUT`, `SERVICE_UNAVAILABLE`, `ADAPTER_NOT_FOUND`, plus handler-layer `POLICY_HOOK_VIOLATION`, `VALIDATION_ERROR`, `DATABASE_ERROR`, `SERIALIZATION_ERROR`, `ACCESS_DENIED`, `ADAPTER_NOT_LOADABLE`, `APPROXIMATE_REPLAY_REQUIRED`. All errors are wrapped in `ApiErrorBody { code, message, detail?, request_id }`.

### Components Status

✅ **Implemented:**
1. **Model Loader** (`crates/adapteros-lora-worker/src/model_loader.rs`)
   - SafeTensors format support
   - LRU caching with eviction
   - Memory estimation
   - Model validation

2. **Inference Pipeline** (`crates/adapteros-lora-worker/src/inference_pipeline.rs`)
   - Complete autoregressive generation
   - K-sparse LoRA routing
   - Policy enforcement
   - Telemetry integration
   - Circuit breaker protection

3. **Worker** (`crates/adapteros-lora-worker/src/lib.rs`)
   - Full inference workflow
   - Hot-swap management
   - Evidence retrieval (RAG)
   - Patch proposal generation
   - GPU verification

4. **Tokenizer** (`crates/adapteros-lora-worker/src/tokenizer.rs`)
   - Qwen2.5 tokenizer
   - Chat template support
   - Encode/decode functions

5. **Router** (`crates/adapteros-lora-router`)
   - K-sparse adapter selection
   - Q15 quantized gates
   - Deterministic seed-based routing

6. **Metal Kernels** (`crates/adapteros-lora-kernel-mtl`)
   - Fused MLP + LoRA operations
   - GPU buffer management
   - Deterministic execution

⚠️ **Needs Integration:**
1. REST API generation endpoint (exists but may need updates)
2. Model runtime management (partially disabled in server-api)
3. End-to-end testing

## Data Flow

```
User Prompt (text)
    ↓
[1] Tokenizer → [token_ids]
    ↓
[2] InferencePipeline.infer() → Autoregressive Loop:
    ├─ [3] Router.route() → Decision { adapter_ids, gates_q15 }
    ├─ [4] HotSwap → Ensure adapters loaded
    ├─ [5] MetalKernels.run_step() → Apply LoRA deltas
    └─ [6] Generator.next_token() → Sample from logits
    ↓
[7] Tokenizer.decode() → Generated Text
    ↓
[8] Build InferenceResponse with trace
```

## Integration Points

### Current Entry Points

1. **Worker.infer()** - Main inference entry point
   - Location: `crates/adapteros-lora-worker/src/lib.rs:555`
   - Accepts: `InferenceRequest`
   - Returns: `InferenceResponse`
   - Features: Memory pressure handling, evidence retrieval, telemetry

2. **InferencePipeline.infer()** - Core pipeline
   - Location: `crates/adapteros-lora-worker/src/inference_pipeline.rs:226`
   - Handles: Tokenization, routing, generation, decoding
   - Policy: Quarantine checks, entropy floor

3. **Hot-Swap Commands** - Adapter management
   - Location: `crates/adapteros-lora-worker/src/lib.rs:1061`
   - Operations: Load, Unload, Swap adapters

### Missing Pieces

1. **REST API Handler** - `/v1/generate` endpoint
   - Status: Needs implementation in `crates/adapteros-server-api/src/handlers.rs`
   - Should: Call Worker.infer() and return JSON response
   - Authentication: RBAC permission check

2. **Model Loading API** - `/v1/models/load` endpoint
   - Status: Partially implemented in `handlers/models.rs`
   - Needs: Integration with Worker initialization

3. **Streaming Support** - SSE for token-by-token generation
   - Status: Architecture exists (mentioned in CLAUDE.md)
   - Needs: Integration with InferencePipeline

## Implementation Gaps

### Gap 1: Model Loading at Server Startup

**Current:** Worker expects model path and tokenizer during construction
**Needed:** Server initialization code to load base model

```rust
// In adapteros-server/src/main.rs (or model_runtime.rs)
let model_path = Path::new("models/qwen2.5-7b");
let tokenizer_path = model_path.join("tokenizer.json");

let kernels = MetalKernels::new()?;
let worker = Worker::new(
    manifest,
    kernels,
    None, // RAG
    tokenizer_path.to_str().unwrap(),
    model_path.to_str().unwrap(),
    telemetry,
).await?;
```

### Gap 2: REST API Handler

**Status:** `adapteros-server-api` crate is disabled due to compilation errors
**Blocker:** Must fix server-api compilation before adding handlers

### Gap 3: Integration Testing

**Exists:** Multiple test files but not end-to-end
- `tests/inference_integration_tests.rs`
- `tests/e2e/inference_pipeline.rs`

**Needed:** Full stack test (HTTP → Worker → Kernels → Response)

## Recommended Approach for PRD #4

### Option A: Fix Server API (High Effort)
1. Fix `adapteros-server-api` compilation (62 errors)
2. Add `/v1/generate` handler
3. Wire up to Worker

**Pros:** Production-ready REST API
**Cons:** Requires fixing all server-api issues first

### Option B: Minimal Integration (MVP Focus)
1. Create standalone binary that uses Worker directly
2. Test via command-line interface (like `aosctl infer`)
3. Demonstrate core functionality without HTTP layer

**Pros:** Faster to demo, focuses on core
**Cons:** No web UI integration

### Option C: Hybrid Approach (Recommended)
1. Extend `aosctl` CLI with `infer` subcommand
2. Create integration test that exercises full pipeline
3. Document HTTP API design for future implementation

**Pros:** Deliverable MVP + clear path forward
**Cons:** Defers full REST API

## Next Steps

1. ✅ Document current architecture (this file)
2. Fix `adapteros-server-api` compilation OR create CLI interface
3. Create end-to-end integration test
4. Write demo script
5. Update QUICKSTART.md

## Key Files Reference

- Worker: `crates/adapteros-lora-worker/src/lib.rs`
- Inference Pipeline: `crates/adapteros-lora-worker/src/inference_pipeline.rs`
- Model Loader: `crates/adapteros-lora-worker/src/model_loader.rs`
- Tokenizer: `crates/adapteros-lora-worker/src/tokenizer.rs`
- Router: `crates/adapteros-lora-router/src/lib.rs`
- Kernels: `crates/adapteros-lora-kernel-mtl/src/lib.rs`
- Server (disabled): `crates/adapteros-server-api/src/handlers.rs`
- CLI: `crates/adapteros-cli/src/main.rs`

## Model Requirements

For MVP, need:
- Qwen2.5-7B model weights (SafeTensors format)
- `model.safetensors` file
- `config.json` file
- `tokenizer.json` file

Can download from HuggingFace:
```bash
# Using huggingface-cli
huggingface-cli download Qwen/Qwen2.5-7B-Instruct \
  --local-dir models/qwen2.5-7b \
  --include "model.safetensors" "config.json" "tokenizer.json"
```

## Base model loading (Dec 2025 update)

- Canonical statuses: `no-model`, `loading`, `ready`, `unloading`, `error`, `checking` (JSON-serialized).
- Aggregation (cluster-level, per model): any `ready` → `ready`; else any `loading` → `loading`; else any `checking` → `checking`; else any `unloading` → `unloading`; else any `error` → `error`; else `no-model`.
- Router guard: inference is allowed only when aggregated status is `ready`; otherwise requests fail fast with `MODEL_NOT_READY` (503) and ApiErrorBody with `request_id`.
- Scope: base models are global; `tenant_id` on status endpoints is a view filter today. Load/unload is admin/operator-only; future per-tenant loading would thread `tenant_id` through status, load, and unload.
- Observability/metrics: load/unload errors surface via ApiErrorBody + `X-Request-ID`; metrics are `adapteros_model_load_success_total`, `adapteros_model_load_failure_total`, `adapteros_model_unload_success_total`, `adapteros_model_unload_failure_total`, and gauge `adapteros_model_loaded{model_id,tenant_id}` (1 = ready).

MLNavigator Inc 2025-12-07.
