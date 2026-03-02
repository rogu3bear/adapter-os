# Phase 7: UI and Developer Experience - Context

**Gathered:** 2026-02-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Polish and verify existing user-facing surfaces against the hardened backend: Leptos web UI, Ratatui terminal dashboard, and `aosctl` CLI. This phase is integration-quality and operator/developer ergonomics work, not backend feature expansion.

</domain>

<decisions>
## Implementation Decisions

### Scope Discipline
- Reuse existing UI/TUI/CLI surfaces and scripts; avoid parallel tooling or duplicate flows.
- Prefer verification and targeted fixes over broad UX refactors.
- Keep backend behavior unchanged unless a UI/TUI/CLI integration defect requires a minimal fix.

### Web UI Acceptance Path (UX-01)
- Treat UI readiness as a pipeline: `adapteros-ui` WASM compile, static asset validation, and foundation-run root path smoke.
- Keep `scripts/foundation-run.sh` as the canonical end-to-end gate for UI availability.
- UI fixes should stay within existing Leptos + Trunk + static asset flow.

### TUI Acceptance Path (UX-02)
- Validate live metrics and adapter status from existing API paths (`/api/metrics`, `/api/adapters`) and TUI update loop.
- Ensure dashboard/status-bar values (latency/TPS/memory/adapter load state) stay coherent with the fetched model.
- Maintain 1-second refresh behavior unless a defect forces change.

### CLI Acceptance Path (UX-03)
- Verify command surface via automation and tests, not manual one-by-one ad-hoc checks.
- Use layered validation: clap parsing/help, representative API-backed command smoke, and output-format assertions (human + JSON paths).
- Preserve existing command taxonomy and aliases; avoid command reshaping in this phase.

### Leptos 0.8 Decision Gate (UX-04)
- Produce an explicit upgrade decision: upgrade now or defer.
- Decision must be grounded in compile/runtime/test evidence and migration risk.
- If deferred, record concrete blockers and re-entry criteria.

### Claude's Discretion
- Exact sequencing of UI/TUI/CLI verification tasks inside the phase.
- Minimal helper scripts/tests to improve repeatable command coverage.
- Whether Leptos 0.8 evaluation is done on a spike branch/worktree before applying any lockfile change.

</decisions>

<specifics>
## Specific Ideas

- Requirements for this phase: `UX-01`, `UX-02`, `UX-03`, `UX-04`.
- Existing evidence anchors:
- Web UI crate is Leptos 0.7 (`crates/adapteros-ui/Cargo.toml`) and built for `wasm32-unknown-unknown` in CI.
- `scripts/foundation-run.sh` already enforces UI asset checks and rebuild fallback.
- TUI app refresh loop already pulls health/metrics/adapters and renders memory/TPS/latency/dashboard state.
- CLI command surface is large (`aosctl`) with existing help/parsing tests in `crates/adapteros-cli/tests`.
- Planning artifact needed for this phase: `07-01-PLAN.md`.

</specifics>

<deferred>
## Deferred Ideas

- New UI features, IA rewrites, or design-system changes unrelated to hardened-backend compatibility.
- New TUI screens beyond metrics/adapter/status requirements.
- CLI command redesigns or namespace migrations.
- Cross-phase backend enhancements (observability/security/operations) outside UX acceptance criteria.

</deferred>

---

*Phase: 07-ui-and-developer-experience*
*Context gathered: 2026-02-24*
