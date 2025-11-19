# AdapterOS Inference Flow

## Current Architecture (As of 2025-01-19)

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
