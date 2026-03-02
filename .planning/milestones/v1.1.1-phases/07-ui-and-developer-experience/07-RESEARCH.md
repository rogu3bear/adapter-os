# Phase 7: UI and Developer Experience - Research

**Researched:** 2026-02-24
**Domain:** Leptos web UI, Ratatui TUI, aosctl command surface
**Confidence:** HIGH

## Summary

Phase 7 is mostly a verification-and-gap-closure phase, not a greenfield build. The web UI already compiles as WASM in CI and is integrated into `foundation-run` via UI asset checks and root-path smoke. The TUI already fetches live health/metrics/adapter data on a refresh loop and renders latency/TPS/memory plus adapter load state. The CLI already has a broad command surface (60 top-level commands in `Commands` enum, plus grouped subcommands) with parsing/help tests.

The key missing deliverable is coherent Phase 7 execution: one plan that validates all three surfaces against the hardened API, closes any integration regressions, and records a definitive Leptos 0.8 upgrade decision.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Reuse existing UI/TUI/CLI surfaces and scripts; no parallel implementation streams.
- UI acceptance must use the existing WASM + assets + foundation-run path.
- TUI acceptance must validate live metrics and adapter state from existing API paths.
- CLI acceptance must be test/script driven, not manual ad-hoc execution.
- Leptos 0.8 requires an explicit documented decision with evidence.

### Claude's Discretion
- Verification sequencing and smallest useful automation additions.
- Minimal tactical fixes in surface crates/scripts to satisfy UX requirements.
- Evaluation method for Leptos 0.8 risk and readiness.

### Deferred Ideas (OUT OF SCOPE)
- Broad visual redesigns or new UX features.
- CLI taxonomy refactors.
- Backend feature additions unrelated to Phase 7 acceptance criteria.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| UX-01 | Leptos web UI compiles and serves from foundation run | `crates/adapteros-ui/Cargo.toml` targets Leptos 0.7 WASM; CI runs wasm checks/build jobs; `scripts/foundation-run.sh` enforces `scripts/ci/check_ui_assets.sh` and rebuild fallback via `scripts/build-ui.sh`. |
| UX-02 | TUI dashboard displays live metrics and adapter status | `crates/adapteros-tui/src/app.rs` refreshes data each second; `src/app/api.rs` fetches `/api/metrics` and `/api/adapters`; `src/ui/dashboard.rs` + `src/ui/status_bar.rs` render latency/TPS/memory/adapter-loaded state. |
| UX-03 | 60+ CLI commands verified against hardened API with formatting | `crates/adapteros-cli/src/app.rs` defines large command surface (`aosctl`); existing tests cover parsing/help (`tests/command_parsing_tests.rs`, `tests/cli_help_golden.rs`) and can be extended with API-backed smoke matrix. |
| UX-04 | Leptos 0.8 upgrade evaluated with documented decision | Current UI dependencies are Leptos/Router 0.7 in `crates/adapteros-ui/Cargo.toml`; no phase-level decision artifact exists yet. |
</phase_requirements>

## Standard Stack

| Surface | Current Stack | Evidence | Risk Profile |
|---------|---------------|----------|--------------|
| Web UI | Leptos 0.7 + Trunk + wasm32 target | `crates/adapteros-ui/Cargo.toml`, CI wasm jobs, `scripts/build-ui.sh` | MEDIUM (framework upgrade + wasm asset compatibility) |
| TUI | Ratatui + reqwest API polling + SSE client | `crates/adapteros-tui/src/app.rs`, `src/app/api.rs`, `src/ui/*` | LOW-MEDIUM (data mapping drift vs API) |
| CLI | Clap-based `aosctl` with broad command tree | `crates/adapteros-cli/src/app.rs`, `crates/adapteros-cli/tests/*` | MEDIUM (coverage breadth and output consistency) |

## Architecture Patterns

### Pattern: Canonical UI serving path
- Build/validate assets with existing scripts, then verify via `foundation-run` and smoke checks.
- Do not introduce a new UI launch path for acceptance.

### Pattern: TUI pulls then renders
- `App::update()` pulls health, metrics, services, adapters, and logs.
- UI components consume app state for dashboard/status visualization.

### Pattern: CLI coverage should be layered
- Parsing/help tests catch interface regressions quickly.
- Selected API-backed smoke checks validate runtime behavior and formatting contracts.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| UI runtime validation | Ad-hoc UI script | `scripts/foundation-run.sh` + `scripts/foundation-smoke.sh` | Already enforces build/boot/smoke invariants |
| CLI command verification | Manual per-command checklist | Existing clap/help/parsing tests + targeted API smoke matrix | Scalable and repeatable |
| Leptos upgrade decision | Opinion-only recommendation | Compile/test/runtime evidence + explicit decision artifact | Prevents speculative churn |

## Common Pitfalls

### Pitfall 1: Declaring UI ready without foundation-run validation
`cargo check` for wasm alone does not prove assets are served correctly from backend static paths.

### Pitfall 2: TUI metrics labels detached from API fields
If API payload shape changes, dashboard/status may silently show defaults; verification must include live payload-path checks.

### Pitfall 3: Counting CLI commands without grouped subcommands
Top-level enum count alone can underrepresent practical surface area; verification should include grouped namespaces.

### Pitfall 4: Leptos 0.8 decision without migration evidence
A blind upgrade or blind defer both create debt; the phase requires a documented, test-backed decision.

## Code Examples

### Targeted command checks
```bash
cargo check -p adapteros-ui --target wasm32-unknown-unknown
cargo run -p adapteros-cli --bin aosctl -- --help
cargo test -p adapteros-cli --test command_parsing_tests
cargo run -p adapteros-tui -- --help
scripts/foundation-run.sh --headless
```

### CLI surface sanity count (top-level)
```bash
awk 'BEGIN{in_enum=0} /pub enum Commands/{in_enum=1;next} in_enum&&/^}/ {in_enum=0} in_enum{if ($0 ~ /^    [A-Z][A-Za-z0-9_]*[[:space:]]*(\{|\(|,)/) {print $1}}' crates/adapteros-cli/src/app.rs | sed 's/[{,(]$//' | sort -u | wc -l
```

## State of the Art

| Requirement Area | Existing State | Gap to Close |
|------------------|----------------|--------------|
| Web UI | WASM build + asset checks + foundation-run integration exist | Validate end-to-end against hardened backend and fix any regressions |
| TUI | Live polling/render pipeline for health/metrics/adapters exists | Confirm required metrics/status fidelity under hardened API |
| CLI | Broad command surface + parsing/help tests exist | Build/execute Phase-7-specific verification matrix tied to hardened API |
| Leptos 0.8 | Still on 0.7 | Produce and document go/no-go decision |

## Current State (Verified)

- `adapteros-ui` is on Leptos 0.7 and configured for wasm target builds.
- CI includes multiple wasm/UI checks and artifact validation steps.
- `foundation-run` performs UI asset validation and fallback rebuild before backend boot.
- TUI update loop fetches metrics/adapters and renders dashboard/status-bar metrics (latency/TPS/memory/adapter load).
- CLI has 60 top-level commands in `Commands` enum plus additional grouped subcommands, with existing help/parsing tests.

