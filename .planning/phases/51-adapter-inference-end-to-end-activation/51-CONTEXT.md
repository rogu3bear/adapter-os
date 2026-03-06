# Phase 51: Adapter Inference End-to-End Activation - Context

**Gathered:** 2026-03-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Make LoRA adapters functional end-to-end: wire the API-side LifecycleManager to the worker-side HotSwapManager over UDS so adapter swaps actually reach the live MLX kernel, verify adapters measurably influence generation, and ensure the training pipeline produces adapters that load and work in inference. This phase closes the gap between "adapter state tracked in DB" and "adapter weights active on GPU."

</domain>

<decisions>
## Implementation Decisions

### Hot-swap behavior
- In-flight inference requests finish with their original adapter (RCU-style, matching the existing `HotSwapManager` retirement design with `AtomicUsize` refcounts)
- New adapter applies to the next request after the swap completes
- The swap is atomic pointer-flip — no partial states, no restart needed
- If swap fails (e.g., corrupt weights), rollback to previous adapter stack (existing `AdapterCommand::Rollback` path)

### API-to-worker swap channel
- Add a UDS command route (e.g., `/adapter/swap`) alongside the existing `/inference` route in `uds_server.rs`
- The API's `POST /v1/adapters/swap` handler sends `AdapterCommand` over UDS to the worker, which dispatches to its `HotSwapManager::execute()`
- This closes the central gap: API-side LifecycleManager updates DB state AND signals worker to actually swap GPU-resident weights
- `lifecycle_manager` should be required (not `Option`) for deployments that serve inference — fail closed if absent

### Adapter influence verification
- Token-level comparison: run the same prompt through base model and adapter-loaded model, assert output token IDs differ
- Use the existing deterministic seed derivation so both runs are reproducible
- This is a test/verification concern, not a runtime feature — implemented as integration tests
- The MLX stub (sine-wave logits) won't work for this; tests need the real MLX backend or a test adapter that produces known-different outputs

### Training-to-inference handoff
- Manual activation via API call (`POST /v1/adapters/swap`) after training completes — no auto-promotion
- Training pipeline packages to `var/adapters/repo/<tenant>/<adapter_id>/` (existing `AdapterPackager` path)
- After packaging, adapter state in DB is updated to "available" (existing `lifecycle_manager.update_adapter_state`)
- User/operator explicitly swaps the adapter into inference when ready
- Auto-promotion is a future capability (separate phase)

### Round-trip validation
- Automated integration test: train a small adapter → package as .aos → load via swap API → infer → assert output differs from base
- Use a tiny synthetic dataset and minimal training steps (speed over quality — this is a wiring test, not a quality test)
- Test runs with real MLX backend on Apple Silicon (not CI-compatible without hardware)
- Deterministic seed pinning ensures the test is reproducible

### Effective adapter resolution before streaming
- `streaming_infer.rs` must resolve `effective_adapter_ids` and ensure adapters are preloaded in the worker's kernel before dispatching the UDS inference request
- Add a preload-before-stream step: resolve adapter stack → send preload command over UDS → then dispatch inference
- If preload fails, return an error before starting the SSE stream (fail fast, not mid-stream)

### Claude's Discretion
- UDS message framing details (length-prefixed JSON, newline-delimited, etc.)
- Exact error types for swap failures
- Whether to add a `/adapter/status` UDS route for querying loaded adapter state
- Test dataset content and training hyperparameters for round-trip test

</decisions>

<specifics>
## Specific Ideas

- The `HotSwapManager` is already fully implemented with two-phase preload/swap/rollback/RCU — this phase wires it up, not reimplements it
- The `AdapterCommand` enum and `AdapterCommandResult` types already exist in the transport types — they just need a UDS dispatch route
- The `ReasoningRouter` per-token adapter swap in `generate_stream_inner` should continue to work once the base hot-swap channel is wired
- Stack hash verification (`compute_stack_hash()`, `compute_cross_layer_hash()`) should be exercised in the round-trip test

</specifics>

<deferred>
## Deferred Ideas

- Auto-promotion of trained adapters into inference (automatic post-training activation)
- Adapter A/B testing (running two adapter stacks and comparing outputs)
- Multi-tenant adapter isolation verification
- Adapter eviction policies (LRU, memory pressure)

</deferred>

---

*Phase: 51-adapter-inference-end-to-end-activation*
*Context gathered: 2026-03-04*
