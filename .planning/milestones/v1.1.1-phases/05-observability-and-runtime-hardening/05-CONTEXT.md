# Phase 5: Observability and Runtime Hardening - Context

**Gathered:** 2026-02-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Harden runtime observability and shutdown behavior so operators can measure inference performance, follow request traces across control plane/worker/kernel boundaries, and trust drain behavior under live traffic. This phase is operational hardening only: no new API feature expansion and no UX polish.

</domain>

<decisions>
## Implementation Decisions

### Metrics Contract First
- Standard `/metrics` and `/v1/metrics` remain the Prometheus scrape surfaces; avoid introducing parallel metrics endpoints.
- Close the requirement gap by ensuring TTFT and TPS are exported as Prometheus histograms, adapter load durations are summarized, and memory pressure gauges are present and updated on runtime cadence.
- Keep metric naming stable and additive where possible to avoid breaking existing dashboards.

### Trace Continuity as Acceptance Gate
- Reuse existing OpenTelemetry + W3C trace-context middleware path as the base; do not fork a second tracing pipeline.
- Acceptance requires one request to produce a connected trace across control plane, worker, and kernel spans (not just local spans in each component).
- Missing cross-boundary links are phase blockers.

### Graceful Shutdown Must Protect Streaming
- Keep current graduated drain + in-flight request accounting model as the primary shutdown mechanism.
- Explicitly validate SIGTERM behavior while streaming inference is active: in-flight streams complete or terminate with deterministic, observable shutdown semantics.
- Rejecting new requests during drain stays mandatory.

### Runtime Memory Hardening Scope
- Integrate a production allocator strategy (mimalloc requirement) without widening into unrelated memory subsystem refactors.
- Wire unified memory + heap accounting into enforcement/backpressure signals that operators can observe.
- Prioritize explicit thresholds and runtime signals over opaque heuristics.

### Claude's Discretion
- Exact metric names for newly added TTFT/TPS instruments if no existing canonical names are enforced.
- Practical trace-link mechanism between control plane and worker/kernel boundaries (header, metadata, or envelope field), as long as it is single-path and verifiable.
- Test harness shape for SIGTERM-on-streaming and memory-budget enforcement validation.

</decisions>

<specifics>
## Specific Ideas

- Requirements in scope: `OBS-01`, `OBS-02`, `OBS-03`, `OBS-04`, `OBS-05`.
- Existing foundations to extend (not replace):
  - Prometheus-compatible metrics endpoint and exporter pipeline
  - OpenTelemetry init + trace-context middleware + inference spans
  - Graduated drain and in-flight request tracking during shutdown
  - UMA/unified memory pressure monitoring primitives
- Phase depends on Phase 4 completion because observability must instrument the finalized API/runtime behavior rather than unstable interfaces.

</specifics>

<deferred>
## Deferred Ideas

- Security hardening and release/provenance work (Phase 6).
- UI/TUI metric presentation and developer UX polish (Phase 7).
- New product telemetry surfaces unrelated to `OBS-01..OBS-05`.

</deferred>

---

*Phase: 05-observability-and-runtime-hardening*
*Context gathered: 2026-02-24*
