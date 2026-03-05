# Phase 51: Adapter Inference End-to-End Activation - Research

**Researched:** 2026-03-04
**Domain:** LoRA adapter hot-swap wiring, inference pipeline activation, training-to-inference handoff
**Confidence:** HIGH

## Summary

Phase 51 closes the gap between "adapter state tracked in DB" and "adapter weights active on GPU." The infrastructure is 90% built — the worker's `HotSwapManager` has two-phase preload/swap/rollback/RCU, the UDS server handles `POST /adapter/command` with the full `AdapterCommand` enum, and the API has `POST /v1/adapters/swap` with preflight checks. The critical missing piece is the **control-plane-to-worker wiring**: the swap API handler only updates DB/lifecycle state but never sends the `AdapterCommand::Swap` over UDS to actually move weights on the GPU. A secondary gap is the **UDS client lacking a JSON-based adapter command method** — it only has `adapter_command(id, command_str)` for simple string commands, not the tagged JSON `AdapterCommand` enum the worker expects on `POST /adapter/command`.

**Primary recommendation:** Wire the API swap handler to send `AdapterCommand` over UDS via a new `UdsClient::send_adapter_command_json()` method, add a preload-before-stream step in `streaming_infer.rs`, and build integration tests proving the round-trip.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- In-flight inference requests finish with their original adapter (RCU-style, matching the existing `HotSwapManager` retirement design with `AtomicUsize` refcounts)
- New adapter applies to the next request after the swap completes
- The swap is atomic pointer-flip — no partial states, no restart needed
- If swap fails (e.g., corrupt weights), rollback to previous adapter stack (existing `AdapterCommand::Rollback` path)
- Add a UDS command route (e.g., `/adapter/swap`) alongside the existing `/inference` route in `uds_server.rs`
- The API's `POST /v1/adapters/swap` handler sends `AdapterCommand` over UDS to the worker, which dispatches to its `HotSwapManager::execute()`
- This closes the central gap: API-side LifecycleManager updates DB state AND signals worker to actually swap GPU-resident weights
- `lifecycle_manager` should be required (not `Option`) for deployments that serve inference — fail closed if absent
- Token-level comparison: run the same prompt through base model and adapter-loaded model, assert output token IDs differ
- Use the existing deterministic seed derivation so both runs are reproducible
- This is a test/verification concern, not a runtime feature — implemented as integration tests
- The MLX stub (sine-wave logits) won't work for this; tests need the real MLX backend or a test adapter that produces known-different outputs
- Manual activation via API call (`POST /v1/adapters/swap`) after training completes — no auto-promotion
- Training pipeline packages to `var/adapters/repo/<tenant>/<adapter_id>/` (existing `AdapterPackager` path)
- After packaging, adapter state in DB is updated to "available" (existing `lifecycle_manager.update_adapter_state`)
- User/operator explicitly swaps the adapter into inference when ready
- Auto-promotion is a future capability (separate phase)
- Automated integration test: train a small adapter → package as .aos → load via swap API → infer → assert output differs from base
- Use a tiny synthetic dataset and minimal training steps (speed over quality — this is a wiring test, not a quality test)
- Test runs with real MLX backend on Apple Silicon (not CI-compatible without hardware)
- Deterministic seed pinning ensures the test is reproducible
- `streaming_infer.rs` must resolve `effective_adapter_ids` and ensure adapters are preloaded in the worker's kernel before dispatching the UDS inference request
- Add a preload-before-stream step: resolve adapter stack → send preload command over UDS → then dispatch inference
- If preload fails, return an error before starting the SSE stream (fail fast, not mid-stream)

### Claude's Discretion
- UDS message framing details (length-prefixed JSON, newline-delimited, etc.)
- Exact error types for swap failures
- Whether to add a `/adapter/status` UDS route for querying loaded adapter state
- Test dataset content and training hyperparameters for round-trip test

### Deferred Ideas (OUT OF SCOPE)
- Auto-promotion of trained adapters into inference (automatic post-training activation)
- Adapter A/B testing (running two adapter stacks and comparing outputs)
- Multi-tenant adapter isolation verification
- Adapter eviction policies (LRU, memory pressure)
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| INF-51-01 | Adapter hot-swap during inference completes without crash or hang | UDS command wiring + HotSwapManager already handles RCU + integration tests |
| INF-51-02 | Inference output with adapter loaded differs measurably from base model output | Deterministic seed + token-level comparison test with real MLX backend |
| TRN-51-01 | Training pipeline produces an adapter that loads and influences inference | AdapterPackager → .aos → lifecycle update to "available" → swap API |
| TRN-51-02 | Round-trip: train → load → infer with adapter produces coherent output | End-to-end integration test exercising full pipeline |
</phase_requirements>

## Standard Stack

### Core (All Existing — No New Dependencies)
| Library/Crate | Location | Purpose | Status |
|---------------|----------|---------|--------|
| `adapteros-lora-worker` | `crates/adapteros-lora-worker/` | Worker: HotSwapManager, AdapterCommand, UDS server | Exists, needs wiring |
| `adapteros-server-api` | `crates/adapteros-server-api/` | API: swap handler, UDS client, streaming_infer | Exists, needs wiring |
| `adapteros-lora-lifecycle` | `crates/adapteros-lora-lifecycle/` | Lifecycle state management | Exists |
| `adapteros-transport-types` | `crates/adapteros-transport-types/` | UDS transport contract types | Exists |
| `adapteros-inference-contract` | `crates/adapteros-inference-contract/` | Inference path constants | Exists |
| `adapteros-core` | `crates/adapteros-core/` | Shared types, B3Hash, seed derivation | Exists |

### No New Dependencies Required
This phase is entirely wiring existing components. No new crates or external dependencies needed.

## Architecture Patterns

### Existing Architecture (What's Already Built)

```
API (AppState)
├── lifecycle_manager: Option<Arc<LifecycleManager>>   ← DB + in-memory state
├── uds_client: UdsClient                              ← sends HTTP-over-UDS
└── swap handler (handlers/adapters/swap.rs)            ← preflight + DB update

Worker (UDS Server)
├── HotSwapManager                                     ← two-phase preload/swap/rollback/RCU
├── AdapterCommand enum                                ← Preload | Swap | Rollback | VerifyStack
├── execute_adapter_command()                           ← dispatches to HotSwapManager
└── UDS route: POST /adapter/command                   ← parses AdapterCommand JSON
```

### Gap: Missing Wiring

```
API swap handler
  ↓ updates DB state ✓
  ↓ updates lifecycle_manager ✓
  ✗ does NOT send AdapterCommand over UDS to worker
  ✗ adapter weights never move on GPU

UdsClient
  ↓ adapter_command(path, id, command_str)              ← simple string, old format
  ✗ no send_adapter_command_json(path, AdapterCommand)  ← JSON enum, worker expects this
```

### Pattern 1: UDS Client JSON Command Method
**What:** Add `UdsClient::send_adapter_command_json()` that sends `AdapterCommand` as JSON body to `POST /adapter/command`
**Why:** Worker's UDS server already parses this route with `serde_json::from_str::<AdapterCommand>(body)`. Client just needs to send matching JSON.
**Example:**
```rust
pub async fn send_adapter_command_json(
    &self,
    uds_path: &Path,
    command: &AdapterCommand,
) -> Result<AdapterCommandResult, UdsClientError> {
    let body = serde_json::to_string(command)?;
    let response = self.send_http_request(uds_path, "POST", "/adapter/command", Some(serde_json::to_value(command)?)).await?;
    serde_json::from_value(response).map_err(|e| UdsClientError::SerializationError(e.to_string()))
}
```

### Pattern 2: Swap Handler → UDS Dispatch
**What:** After preflight + DB update, swap handler sends `AdapterCommand::Swap` over UDS
**Where:** `crates/adapteros-server-api/src/handlers/adapters/swap.rs` after lifecycle manager updates
**Key consideration:** Worker discovery — need `state.worker` to get UDS path

### Pattern 3: Preload-Before-Stream
**What:** Before dispatching inference over UDS, resolve effective adapters and send `AdapterCommand::Preload` for any not yet loaded
**Where:** `crates/adapteros-server-api/src/handlers/streaming_infer.rs`, before the UDS inference dispatch
**Key consideration:** Fail fast — if preload fails, return HTTP error before starting SSE stream

### Anti-Patterns to Avoid
- **Direct GPU manipulation from API:** Never bypass the worker's HotSwapManager. Always go through UDS → AdapterCommand → HotSwapManager.
- **Fire-and-forget swap:** Always wait for AdapterCommandResult confirmation before telling client "swap succeeded."
- **Preload during stream:** Never start SSE and then discover adapter isn't loaded. Preload BEFORE the stream.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Adapter swap atomicity | Custom locking | `HotSwapManager` (already built) | Has RCU refcounts, rollback, generation tracking |
| UDS framing | New protocol | `send_http_request()` on UdsClient | Already handles HTTP-over-UDS framing |
| Stack hash verification | Manual hash | `compute_stack_hash()` / `compute_cross_layer_hash()` | Already exists in adapter_hotswap.rs |
| Adapter file resolution | Custom lookup | `resolve_adapter_file()` | Handles flat .aos files and directory layouts |
| Preflight validation | Custom checks | `run_api_preflight()` | Already validates hash, manifest, lifecycle state, evidence |

## Common Pitfalls

### Pitfall 1: lifecycle_manager is Option
**What goes wrong:** Swap handler silently falls back to DB-only updates when `lifecycle_manager` is None, adapter never reaches GPU.
**Why it happens:** `AppState.lifecycle_manager: Option<Arc<LifecycleManager>>` — the Option exists for test/dev modes.
**How to avoid:** For swap operations, require lifecycle_manager. Return error if absent instead of falling back.
**Warning signs:** Swap returns success but adapter behavior doesn't change.

### Pitfall 2: Worker UDS path discovery
**What goes wrong:** API doesn't know the worker's UDS socket path when trying to send adapter commands.
**Why it happens:** The inference path resolves worker via registered worker query from DB, but swap handler doesn't do this.
**How to avoid:** Use same worker resolution as inference handler — query registered workers, get UDS path.
**Warning signs:** "Connection refused" or "No such file" errors on UDS connect.

### Pitfall 3: AdapterCommand type not shared
**What goes wrong:** UDS client tries to serialize a different version of AdapterCommand than worker expects.
**Why it happens:** `AdapterCommand` is defined in `adapteros-lora-worker` which the API crate can't depend on directly (it would create a circular dependency).
**How to avoid:** Either re-export AdapterCommand through `adapteros-transport-types` or define a mirror type in the API crate that serializes identically. The `#[serde(tag = "type", rename_all = "snake_case")]` format must match exactly.
**Warning signs:** "Failed to parse AdapterCommand" errors in worker logs.

### Pitfall 4: Preload race with concurrent inferences
**What goes wrong:** Multiple concurrent inference requests try to preload the same adapter simultaneously.
**Why it happens:** No dedup on preload commands.
**How to avoid:** The HotSwapManager already handles this — `preload_adapter()` checks if adapter is already in the active set. The API just needs to send the command; the worker is idempotent.
**Warning signs:** Duplicate VRAM allocations (check memory reports).

### Pitfall 5: Integration tests need real MLX
**What goes wrong:** Tests pass on stub backend but don't prove adapter influence.
**Why it happens:** Sine-wave logits are deterministic regardless of adapter — no way to distinguish.
**How to avoid:** Gate integration tests behind `#[cfg(feature = "extended-tests")]` or `#[ignore]` with clear docs. Provide a known-good test adapter .aos file that produces different outputs.
**Warning signs:** "All tests pass" but adapter never actually influences output.

## Code Examples

### Existing: Worker executes adapter command (adapter_operations.rs)
```rust
pub async fn execute_adapter_command(
    &mut self,
    command: AdapterCommand,
) -> Result<AdapterCommandResult> {
    // Memory pressure check for Preload
    // Dispatches to HotSwapManager for Swap, Rollback, VerifyStack
    // Returns AdapterCommandResult { success, message, stack_hash, ... }
}
```

### Existing: UDS server routes /adapter/command
```rust
if parts.len() == 3 && parts[2] == "command" {
    use crate::adapter_hotswap::AdapterCommand;
    let command: AdapterCommand = parse_json_with_limit(&request.body)?;
    let result = worker_guard.execute_adapter_command(command).await;
    Self::send_json_response(&mut stream, result).await?;
}
```

### Existing: Swap handler API (handlers/adapters/swap.rs)
```rust
pub async fn swap_adapters(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<AdapterSwapRequest>,
) -> ApiResult<AdapterSwapResponse> {
    // 1. Permission check ✓
    // 2. Fetch both adapters ✓
    // 3. Preflight on new adapter ✓
    // 4. lifecycle_manager.evict_adapter(old) ✓
    // 5. lifecycle_manager.get_or_reload_async(new) ✓
    // 6. Update states ✓
    // 7. ✗ MISSING: Send AdapterCommand::Swap over UDS
}
```

### Existing: UDS client simple command
```rust
pub async fn adapter_command(
    &self, uds_path: &Path, adapter_id: &str, command: &str,
) -> Result<(), UdsClientError> {
    // Sends: POST /adapter/{adapter_id}/{command} HTTP/1.1
    // Only returns Ok/Err, doesn't parse response body
}
```

## State of the Art

| Component | Current State | Needed State | Gap Size |
|-----------|--------------|--------------|----------|
| HotSwapManager | Fully implemented (preload/swap/rollback/RCU/verify) | No changes needed | None |
| AdapterCommand enum | Fully implemented (Preload/Swap/Rollback/VerifyStack) | No changes needed | None |
| UDS server /adapter/command | Fully implemented (JSON dispatch) | No changes needed | None |
| UDS client | Simple string commands only | Needs JSON AdapterCommand sender | Small |
| API swap handler | DB/lifecycle only | Needs UDS dispatch to worker | Medium |
| streaming_infer.rs | Sets effective_adapter_ids | Needs preload-before-stream | Medium |
| Integration tests | Hotswap unit/stress tests exist | Needs end-to-end swap + inference test | Medium |
| Training round-trip test | None | Needs train → package → swap → infer test | Medium |

## Open Questions

1. **AdapterCommand type sharing across crates**
   - What we know: AdapterCommand is in `adapteros-lora-worker`, API crate can't depend on worker crate
   - What's unclear: Should we move AdapterCommand to `adapteros-transport-types` or use raw JSON in the UDS client?
   - Recommendation: Move `AdapterCommand` and `AdapterCommandResult` to `adapteros-transport-types` since they represent the UDS transport contract. This is the cleanest solution. If that creates too many downstream changes, define a mirror serialization in the UDS client using raw `serde_json::Value`.

2. **Worker UDS path resolution in swap handler**
   - What we know: Inference handler resolves worker via DB query (`registered_workers` table)
   - What's unclear: Should swap handler share the same resolution or use a different mechanism?
   - Recommendation: Use `state.worker` handle if available, or query registered workers the same way inference does.

3. **Test adapter for influence verification**
   - What we know: Stub backend can't differentiate base vs adapter output
   - What's unclear: What's the simplest test adapter that produces reliably different output?
   - Recommendation: Create a minimal .aos file with known weight values. Under real MLX, even random small weights will shift logits measurably. Under stub, skip the influence assertion (test the wiring, not the math).

## Sources

### Primary (HIGH confidence)
- Codebase analysis of `crates/adapteros-lora-worker/src/adapter_hotswap.rs` — HotSwapManager, AdapterCommand enum
- Codebase analysis of `crates/adapteros-lora-worker/src/uds_server.rs` — UDS route handling for /adapter/command
- Codebase analysis of `crates/adapteros-server-api/src/handlers/adapters/swap.rs` — current swap handler implementation
- Codebase analysis of `crates/adapteros-server-api/src/uds_client.rs` — current UDS client capabilities
- Codebase analysis of `crates/adapteros-server-api/src/handlers/streaming_infer.rs` — effective_adapter_ids usage
- Codebase analysis of `crates/adapteros-server-api/src/state.rs` — AppState.lifecycle_manager as Option

### Secondary (MEDIUM confidence)
- Codebase analysis of `crates/adapteros-lora-worker/src/adapter_operations.rs` — execute_adapter_command implementation

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all components exist in codebase, verified via code reading
- Architecture: HIGH — gaps precisely identified by tracing data flow
- Pitfalls: HIGH — derived from code patterns and type signatures

**Research date:** 2026-03-04
**Valid until:** 2026-04-04 (stable — internal codebase, not external dependencies)
