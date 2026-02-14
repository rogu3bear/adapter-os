# Phase 1 Evidence Index (Read-Only Scan)

## Method
- Scope: Runtime/Determinism, Control Plane/API, UI, Docs/Scripts.
- Rule: existing-code-first; proposals must extend existing modules and generated-contract flows.
- Caution: +10 (proof-over-assumption, deterministic safety, no duplicate surfaces).

## Team A: Runtime + Determinism

### Existing capabilities
- Deterministic seeding and tie-break context already exist in `crates/adapteros-core/src/seed.rs` (`derive_seed`, `derive_seed_typed`) and `crates/adapteros-core/src/determinism.rs` (`DeterminismContext`, `derive_router_tiebreak_seed`).
- Tick ledger and replay-grade consistency primitives are implemented in `crates/adapteros-deterministic-exec/src/global_ledger.rs` (`GlobalTickLedger`, `record_tick`, `ConsistencyReport`).
- Deterministic multi-agent barrier and thread seed propagation are implemented in `crates/adapteros-deterministic-exec/src/multi_agent.rs` (`AgentBarrier`) and `crates/adapteros-deterministic-exec/src/seed.rs` (`ThreadSeed`).
- Router determinism and quantization invariants are present in `crates/adapteros-lora-router/src/router.rs` (`route_with_adapter_info_and_scope_with_ctx`) and `crates/adapteros-lora-router/src/quantization.rs` (`ROUTER_GATE_Q15_DENOM = 32767.0`).

### Duplicate-work risks
- Seed-state logic appears in both `crates/adapteros-core/src/seed.rs` and `crates/adapteros-deterministic-exec/src/seed.rs`; new seed helpers can easily diverge unless shared/bridged.
- Determinism telemetry has parallel definitions across `crates/adapteros-core/src/telemetry.rs` and `crates/adapteros-deterministic-exec/src/global_ledger.rs`; adding new counters in only one path will fragment observability.

### Reuse-first opportunities
- Reuse `DeterminismContext` beyond router-only call paths so seed derivation remains canonical.
- Reuse `GlobalTickLedger` as the canonical read surface for consistency diagnostics rather than custom SQL views.
- Reuse `AgentBarrier` semantics for tick-based coordination in other runtime synchronization paths.

## Team B: Control Plane + API

### Existing capabilities
- Boot phase orchestration with explicit phase transitions is centralized in `crates/adapteros-server/src/main.rs` and boot modules under `crates/adapteros-server/src/boot/`.
- AppState composition and shared service wiring are centralized in `crates/adapteros-server/src/boot/app_state.rs` and `crates/adapteros-server-api/src/state.rs`.
- Long-running maintenance orchestration exists in `crates/adapteros-server/src/boot/background_tasks.rs` (`spawn_all_background_tasks`).
- Route tiering and middleware stack are centralized in `crates/adapteros-server-api/src/routes/mod.rs` (health/public/optional/internal/protected).
- Readiness and canary behavior are implemented in `crates/adapteros-server-api/src/handlers/health.rs` (`ready`, `run_canary_probe`).

### Duplicate-work risks
- Configuration wrappers are split between `crates/adapteros-server/src/config.rs` and `crates/adapteros-server-api/src/config.rs`; field drift risk is high.
- Boot invariant semantics appear in both boot code (`validate_boot_invariants`) and readiness checks (`ready` handler), risking inconsistent "ready vs degraded" policy.

### Reuse-first opportunities
- Reuse `BackgroundTaskTracker` (`crates/adapteros-server-api/src/state.rs`) as the single status source for task-health APIs/UI.
- Reuse `SseEventManager` (`crates/adapteros-server-api/src/sse/event_manager.rs`) for boot/background progress events.
- Reuse `ProgressService` (`crates/adapteros-server-api/src/progress_service.rs`) for migration/background progress instead of bespoke status channels.

## Team C: UI + Static Surfaces

### Existing capabilities
- Route topology and shell boundaries are centralized in `crates/adapteros-ui/src/lib.rs` with explicit public/protected segmentation.
- Shell-wide SSE subscriptions and keyboard/global UX controls are centralized in `crates/adapteros-ui/src/components/layout/shell.rs`.
- Search + command palette stack is structured across `crates/adapteros-ui/src/signals/search.rs`, `crates/adapteros-ui/src/search/contextual.rs`, and `crates/adapteros-ui/src/components/command_palette.rs`.
- Nav metadata source of truth exists in `crates/adapteros-ui/src/components/layout/nav_registry.rs`.
- Workspace layout primitives are already reusable in `crates/adapteros-ui/src/components/workspace.rs`.
- Diagnostic static surfaces exist in `crates/adapteros-server/static/index.html` and `crates/adapteros-server/static-minimal/`.

### Duplicate-work risks
- Route strings and contextual action routing overlap between `crates/adapteros-ui/src/components/layout/nav_registry.rs` and `crates/adapteros-ui/src/search/contextual.rs`.
- Command metadata and page nav metadata can drift between `crates/adapteros-ui/src/search/index.rs` and nav registry.
- Boot/panic scaffolding duplication exists between `crates/adapteros-server/static/index.html` and `crates/adapteros-server/static-minimal/index-minimal.html`.

### Reuse-first opportunities
- Reuse `all_nav_items` from nav registry for breadcrumbs/sidebar/palette parity.
- Reuse `SearchContext` + providers for global quick-actions and on-page search.
- Reuse workspace primitives for detail pages to avoid ad-hoc grids.

## Team D: Docs + Scripts + Contracts

### Existing capabilities
- Canonical source mapping is already formalized in `docs/CANONICAL_SOURCES.md`.
- Verified evidence snapshots exist in `docs/VERIFIED_REPO_FACTS.md` and route inventory artifacts in `docs/generated/`.
- Contract artifact generator/check exists in `scripts/contracts/generate_contract_artifacts.py` and `scripts/contracts/check_docs_claims.sh`.
- OpenAPI drift gate is codified in `scripts/ci/check_openapi_drift.sh` with deterministic toolchain/version guard.
- CI compliance checks are broad and modular under `scripts/ci/` (route coverage, policy registry, UI assets, stability checks).

### Duplicate-work risks
- Legacy-style doc validators (e.g., `scripts/validate-docs.sh`) duplicate validation responsibilities that now exist in contract checks and can create conflicting signals.
- Multiple plan/audit docs can diverge unless linked to canonical artifacts (`docs/generated/*.json`) and explicit source files.

### Reuse-first opportunities
- Extend `scripts/contracts/generate_contract_artifacts.py` instead of adding new scanners for API/UI/middleware inventories.
- Use `scripts/ci/check_openapi_drift.sh` as the canonical API-contract enforcement point.
- Anchor new research claims in `docs/CANONICAL_SOURCES.md` and `docs/VERIFIED_REPO_FACTS.md` to avoid repeated ad-hoc audits.

## Cross-Team Duplicate Work Risks
- `R1` Seed derivation logic split across core and deterministic-exec.
- `R2` Determinism telemetry vocabulary split across core and deterministic-exec.
- `R3` Config defaults split across server and server-api.
- `R4` Boot-readiness invariant checks duplicated between boot and readiness handlers.
- `R5` UI route metadata and contextual-action routing split.
- `R6` Static boot diagnostics duplicated between static and static-minimal assets.
- `R7` Documentation validation duplicated between contract checks and legacy validators.

## Reuse-First Opportunity Backlog (Evidence-Linked)
- `O1` Build a single seed-state bridge layer by extending `crates/adapteros-core/src/seed.rs` and adapting deterministic-exec callers.
- `O2` Build task-health API from `BackgroundTaskTracker` instead of new storage.
- `O3` Build nav/palette parity by driving contextual actions from nav registry metadata.
- `O4` Build shared static boot diagnostics include/snippet from existing `static` and `static-minimal` templates.
- `O5` Build docs-claims checks by extending `scripts/contracts/generate_contract_artifacts.py` and `scripts/contracts/check_docs_claims.sh`.

