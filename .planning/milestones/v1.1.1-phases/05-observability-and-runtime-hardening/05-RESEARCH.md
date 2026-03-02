# Phase 5: Observability and Runtime Hardening - Research

**Researched:** 2026-02-24
**Domain:** Runtime observability, distributed tracing, graceful shutdown, allocator/memory enforcement
**Confidence:** HIGH

## Summary

The codebase already contains strong observability primitives, but Phase 5 acceptance gaps remain.

What is already present:
- Prometheus-compatible metrics routes (`/metrics`, `/v1/metrics`) and renderer wiring exist.
- OpenTelemetry initialization and W3C trace-context middleware are implemented.
- Graceful shutdown uses in-flight tracking plus a graduated drain model.
- Memory pressure and unified memory tracking infrastructure exists.

What still blocks full `OBS-01..OBS-05` completion:
1. TTFT histogram export is not explicit in current Prometheus metric surfaces.
2. End-to-end trace connectivity across control plane -> worker -> kernel is not yet proven by a single validation path.
3. SIGTERM during active streaming needs explicit acceptance evidence (stream completion/drain behavior under load).
4. mimalloc is not currently configured as the process allocator.
5. Memory budget enforcement exists in pieces but needs explicit runtime enforcement + observability alignment for MLX unified memory and heap.

**Primary recommendation:** Execute Phase 5 as one focused plan (`05-01`) with five tasks aligned 1:1 to `OBS-01..OBS-05`, reusing existing metrics/tracing/shutdown/memory subsystems instead of introducing new pipelines.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Reuse existing `/metrics` surfaces and avoid parallel endpoint creation.
- Reuse existing OpenTelemetry + trace-context path; no duplicate tracing stack.
- Keep graduated drain and in-flight accounting as shutdown foundation.
- Add allocator and memory enforcement with minimal structural change.

### Claude's Discretion
- Exact naming for any new TTFT/TPS metrics where current naming is not canonical.
- Trace-link implementation details at control-plane/worker/kernel boundaries.
- Validation harness details for streaming SIGTERM and memory enforcement.

### Deferred Ideas (OUT OF SCOPE)
- Phase 6 security/release operations.
- Phase 7 UX/dashboard polish.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| OBS-01 | `/metrics` exports TTFT + TPS histograms, adapter load summaries, memory pressure gauges | `/metrics` route and rendering exist; memory pressure gauges and load/worker timing histograms exist; explicit TTFT histogram wiring remains to be completed/validated. |
| OBS-02 | One request yields connected OTel trace across control plane, worker, kernel | OTel init + trace-context middleware + inference spans exist; cross-boundary continuity evidence still needs explicit validation path. |
| OBS-03 | SIGTERM during streaming drains in-flight responses before exit | In-flight request tracking + drain rejection + graduated drain and coordinated shutdown exist; streaming-specific SIGTERM acceptance still needs dedicated verification. |
| OBS-04 | Process runs with mimalloc and exposes allocator evidence | No global allocator/mimalloc integration found in current code search; this is a direct gap. |
| OBS-05 | Memory budget enforcement tracks MLX unified memory + Rust heap with backpressure/alerts | Unified memory trackers, pressure levels, UMA monitor, and memory-pressure metrics exist; end-to-end enforcement + operator-visible thresholds require consolidation/verification. |
</phase_requirements>

## Architecture Patterns

### Pattern 1: Prometheus Surface Is Centralized
- Metrics routes are explicitly mounted at `/metrics` and `/v1/metrics` in [routes/mod.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/routes/mod.rs:1160).
- `metrics_handler` gates on config, updates DB-backed metrics, then renders exporter output in [handlers.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers.rs:3041).
- Boot initializes UDS metrics exporter and periodic publishers in [boot/metrics.rs](/Users/star/Dev/adapter-os/crates/adapteros-server/src/boot/metrics.rs:57).

### Pattern 2: Trace Plumbing Already Exists
- OTel lifecycle and exporter setup is in [otel.rs](/Users/star/Dev/adapter-os/crates/adapteros-server/src/otel.rs:39) and activated in [logging.rs](/Users/star/Dev/adapter-os/crates/adapteros-server/src/logging.rs:521).
- W3C `traceparent` extraction/injection is in [trace_context.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/middleware/trace_context.rs:24).
- Request observability includes trace fields and response headers in [observability.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/middleware/observability.rs:42).
- Inference-level spans are created in [inference_core/core.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/inference_core/core.rs:221).

### Pattern 3: Shutdown and Drain Are Structured
- Server uses `with_graceful_shutdown(...)` in [boot/server.rs](/Users/star/Dev/adapter-os/crates/adapteros-server/src/boot/server.rs:293).
- Graduated drain phases and forced timeout behavior are in [shutdown.rs](/Users/star/Dev/adapter-os/crates/adapteros-server/src/shutdown.rs:154).
- In-flight tracking middleware is in [middleware_security.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/middleware_security.rs:897).
- Drain rejection for new requests during shutdown is in [middleware_security.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/middleware_security.rs:927).

### Pattern 4: Memory Observability Is Partially Wired
- Prometheus critical metrics include memory pressure gauges and worker timing histograms in [critical_components.rs](/Users/star/Dev/adapter-os/crates/adapteros-telemetry/src/metrics/critical_components.rs:61).
- Unified memory pressure classification and strategies exist in [unified_tracker.rs](/Users/star/Dev/adapter-os/crates/adapteros-memory/src/unified_tracker.rs:82).
- Pre-allocation budget model exists in [unified_memory.rs](/Users/star/Dev/adapter-os/crates/adapteros-memory/src/unified_memory.rs:401).
- UMA pressure monitor starts during boot in [boot/metrics.rs](/Users/star/Dev/adapter-os/crates/adapteros-server/src/boot/metrics.rs:326).

## Gaps and Risks

### Gap 1: TTFT metric contract
- TTFT exists as diagnostic data (`ttft_us`) but not clearly as required Prometheus histogram export for Phase 5 acceptance.
- Risk: dashboards appear complete while requirement `OBS-01` is formally unmet.

### Gap 2: Cross-component trace linkage proof
- Local spans and context propagation exist, but there is no single explicit acceptance test proving one connected trace through control plane, worker, and kernel.
- Risk: trace fragmentation under real request paths.

### Gap 3: Streaming SIGTERM acceptance evidence
- Drain model is robust, but no single targeted verification is tied to active streaming shutdown completion semantics.
- Risk: regression where non-streaming drains pass while streaming sessions break.

### Gap 4: Allocator requirement not implemented
- Repository search did not find `mimalloc` or a global allocator binding.
- Risk: `OBS-04` blocked regardless of other progress.

### Gap 5: Enforcement/observability join for memory budgets
- Budget, pressure, and metrics subsystems exist but need explicit linking and alert/backpressure acceptance criteria.
- Risk: memory metrics report state without enforcing runtime behavior.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Prometheus route surface | New metrics endpoint family | Existing `/metrics` + `/v1/metrics` handlers/routes | Already integrated with auth/policy layers and exporter rendering |
| Tracing substrate | Alternate tracing stack | Existing OTel init + trace-context middleware + tracing spans | Preserves existing log/trace correlation path |
| Shutdown orchestration | New ad-hoc signal loop | Existing `shutdown_signal_with_drain` + coordinator lifecycle | Already encodes graduated drain semantics and timeouts |
| Memory pressure policy | Duplicate trackers | Existing unified tracker + UMA monitor + critical metrics | Minimizes drift between memory enforcement and observability |

## Suggested Verification Set (Smallest Relevant)

1. `cargo test -p adapteros-server-api --test streaming_infer` (streaming behavior baseline).
2. A targeted SIGTERM integration run during active streaming (existing shutdown path + in-flight counter).
3. Metrics scrape assertion against `/metrics` confirming TTFT/TPS histograms and memory gauges.
4. OTel trace assertion for one inference request across control-plane/worker/kernel span chain.
5. Allocator startup evidence check (post-mimalloc integration) via startup log/metric marker.

## Open Questions

1. Which existing metric name conventions should TTFT histogram follow to avoid dashboard churn?
2. What is the authoritative correlation key between worker/kernel spans and control-plane inference span (request_id vs trace_id vs both)?
3. Should allocator evidence be emitted as startup log only, metrics gauge only, or both?

## Sources

### Primary (HIGH confidence)
- Metrics routes and middleware layering: [routes/mod.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/routes/mod.rs:1160), [routes/mod.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/routes/mod.rs:2813)
- Metrics handler/render path: [handlers.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers.rs:3041)
- OTel init and wiring: [otel.rs](/Users/star/Dev/adapter-os/crates/adapteros-server/src/otel.rs:39), [logging.rs](/Users/star/Dev/adapter-os/crates/adapteros-server/src/logging.rs:521)
- Trace context middleware: [trace_context.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/middleware/trace_context.rs:24)
- Observability middleware trace fields: [observability.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/middleware/observability.rs:42)
- Inference span creation: [inference_core/core.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/inference_core/core.rs:221)
- Graceful shutdown serving/drain: [boot/server.rs](/Users/star/Dev/adapter-os/crates/adapteros-server/src/boot/server.rs:293), [shutdown.rs](/Users/star/Dev/adapter-os/crates/adapteros-server/src/shutdown.rs:154)
- In-flight/drain middleware: [middleware_security.rs](/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/middleware_security.rs:897)
- Critical metrics and memory pressure primitives: [critical_components.rs](/Users/star/Dev/adapter-os/crates/adapteros-telemetry/src/metrics/critical_components.rs:61), [unified_tracker.rs](/Users/star/Dev/adapter-os/crates/adapteros-memory/src/unified_tracker.rs:82), [unified_memory.rs](/Users/star/Dev/adapter-os/crates/adapteros-memory/src/unified_memory.rs:401), [boot/metrics.rs](/Users/star/Dev/adapter-os/crates/adapteros-server/src/boot/metrics.rs:326)

### Repository search results (HIGH confidence)
- Mimalloc/global allocator search did not find integration points (`global_allocator`, `mimalloc`, `MiMalloc`), indicating an `OBS-04` gap.

