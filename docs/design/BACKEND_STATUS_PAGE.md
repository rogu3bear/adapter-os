# Backend Status Page (UI) Design

## Goals
- Provide a single, authoritative backend status surface that is fully wired to live APIs.
- Surface operational readiness, inference blockers, and model runtime health at a glance.
- Offer drill-down into tenants, stacks, adapters, services, and model inventory without stubs.
- Keep the UI deterministic, paginated, and resilient to partial backend outages.

## Non-Goals
- No new backend endpoints unless existing APIs cannot support the view.
- No major refactor of unrelated UI pages.
- No changes to RBAC or auth flows.

## Data Sources (Existing)
- `/v1/system/status` (SystemStatusResponse)
- `/v1/system/state` (SystemStateResponse)
- `/v1/metrics/system` (SystemMetricsResponse)
- `/v1/stream/workers` (SSE worker updates)
- `/v1/workers`, `/v1/nodes`
- `/v1/models/status/all` (AllModelsStatusResponse)
- `/healthz` (HealthResponse)

## Page Structure
1. Status Overview
   - Readiness, DB, inference, workers, model inventory (loaded/total), and active model.
2. Workers and Nodes
   - SSE-driven worker status with polling fallback.
3. System State
   - Tenants table (status, active stack, counts, memory).
   - Node services table (health + last check).
   - Top adapters by memory (from state summary).
   - RAG status when present (enabled/disabled + reason).
4. Model Runtime
   - All model statuses from `/v1/models/status/all`.
   - Load state, memory usage, timestamps, error details.
5. Metrics + Inference Blockers + Boot
   - Metrics summary (with fallback to kernel memory info).
   - Explicit blockers and boot diagnostics.

## Status Center (Overlay)
- Always backed by `/v1/system/status` + `/v1/system/state`.
- Accessible via Ctrl+Shift+S and the system tray health indicator.

## Behavior and Resilience
- Poll `/v1/system/status`, `/v1/nodes`, `/v1/metrics/system`, `/v1/system/state`, `/v1/models/status/all` every 30s.
- SSE provides worker updates; if disconnected, fallback to polling.
- Permission errors (403) should render a concise notice instead of empty tables.
- UI should distinguish between "unknown" vs "unavailable" states (no fabricated data).

## Implementation Notes
- Keep changes localized to `crates/adapteros-ui/src/pages/system/*` and status tray components.
- Reuse existing `Badge`, `StatusIndicator`, `Card`, `Table`, and `LoadingState` patterns.
- Avoid new abstractions unless repeated logic becomes hard to read.
