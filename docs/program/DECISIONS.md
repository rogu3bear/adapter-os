# AdapterOS Program Decisions

Lightweight ADR-style log.

## D001: Error handling strategy — convert in-place, don't restructure

**Date:** 2026-02-06
**Context:** 18 error sites use raw `.to_string()`, 6 use the unified handler.
**Decision:** Route all sites through `report_error_with_toast()` without changing the error infrastructure itself. The existing `ApiError.user_message()` mapping is sufficient.
**Rationale:** The infrastructure is good; adoption is the problem. Minimal change, maximum consistency.
**Consequence:** Each error site gets a 2-3 line change. No new components needed.

## D002: `env::set_var` replacement — config struct, not env mutation

**Date:** 2026-02-06
**Context:** `std::env::set_var` is unsafe under parallel test execution (Rust 1.74+). Used in test_harness.rs and cleanup.rs.
**Decision:** Pass config through function parameters or test-local structs instead of mutating process-global env.
**Rationale:** Env mutation is fundamentally incompatible with parallel testing. Config passing is deterministic.
**Consequence:** Test harness API changes; individual test files may need minor updates.

## D003: sccache — document, don't remove

**Date:** 2026-02-06
**Context:** sccache is a hard build dependency (`.cargo/config.toml` rustc-wrapper) but undocumented.
**Decision:** Add to prerequisites documentation. Do not add conditional fallback logic.
**Rationale:** sccache provides 90% cache hit rate. Removing it degrades everyone's DX. Documenting is simpler and safer than conditional config.
**Consequence:** CLAUDE.md prerequisites updated.

## D004: First-run detection — use SystemStatusResponse, not new endpoint

**Date:** 2026-02-07
**Context:** Need to detect first-run state to redirect to `/welcome`. Options: new `/v1/first-run` endpoint, or derive from existing `SystemStatusResponse`.
**Decision:** Derive from existing data. First-run = `inference_blockers` contains `NoModelLoaded` AND `WorkerMissing`.
**Rationale:** `SystemStatusResponse` already returns blockers, workers, models. Adding a new endpoint adds API surface for a boolean that can be computed client-side. Dashboard already fetches this data.
**Consequence:** No backend changes needed for B1. Client-side redirect in Dashboard component.

## D005: Nav collapse — 8 groups to 5, merge by user mental model

**Date:** 2026-02-07
**Context:** 8 nav groups with 22 items overwhelm new users. Target: 5 groups matching user workflow.
**Decision:** Merge into: Chat, Data (+ Train), Adapters (+ Route), Observe (+ Govern), Settings (was Org). All routes preserved.
**Rationale:** Users think in workflows: "I want to chat", "I need data", "I want adapters", "I want to monitor", "I need settings." Training is a data operation. Routing is adapter-adjacent. Governance is an observation/compliance function. No pages deleted.
**Consequence:** nav_registry.rs restructured. Alt shortcuts renumbered 1-5. Taskbar shows 5 buttons.
