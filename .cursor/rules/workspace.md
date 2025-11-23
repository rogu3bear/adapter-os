# Workspace Orientation for Cursor Agents

## Primary Code Roots
- `crates/` – all Rust workspace crates (API, orchestrator, worker, telemetry, etc.).
- `docs/` – living design docs; start with `README.md` and `docs/Training.md` for CLI/pipeline context.
- `tests/` – integration and determinism suites; check comments before editing.
- `ui/` – React/TypeScript frontend plus build/test harnesses.
- `xtask/`, `scripts/`, `training/` – internal tooling, datasets, and helper CLIs.

## Heavy Artifacts to Skip
- `adapters/`, `artifacts/`, `plan/`, `models/`, `metal/`, `manifests/`, `var/` – contain binaries, databases, drift reports, or generated bundles.
- `registry.db*`, `server*.log`, coverage outputs – useful for ops but not source changes.
- Numerous `AOS_*`, `FINAL_*`, and other audit reports in the repo root document history; they are static references, not live specs.

## Current Tree Caveats
- The working tree contains massive pending changes (entire subsystems removed or rewritten). Verify intent before deleting or relying on a missing module.
- `cargo test` currently fails early due to the inline-table error in `crates/adapteros-server/Cargo.toml`; address that before expecting clean test runs.
- Many TODO/FIXME markers remain across CLI, server API, and orchestrator code—treat stubs as incomplete unless a linked plan says otherwise.

## Recommended First Steps
1. Run `cargo check --workspace` to confirm the workspace still builds after edits.
2. After fixing the server manifest issue, run targeted suites such as `cargo test -p adapteros-cli --lib`.
3. Use `rg` for search (`rg TODO`, `rg unimplemented!`, etc.) when auditing unfinished features.
4. Consult `README.md` and `docs/Training.md` for end-to-end CLI + training workflows.

## Helpful References
- `IMPLEMENTATION_PLAN.md`, `COMPREHENSIVE_PATCH_PLAN.md` – detail earlier refactors.
- `docs/aos/` – adapter packaging/loading specs.
- `tests/server_api_integration.rs`, `crates/adapteros-server-api/src/handlers.rs` – examples of API surface and remaining stubs.

## Duplication Prevention
- **CRITICAL:** Always prevent code duplication. See `docs/DUPLICATION_PREVENTION_GUIDE.md` for comprehensive guidelines.
- **Quick reference:** `docs/DUPLICATION_PREVENTION_SUMMARY.md`
- **Agent guidelines:** `AGENTS.md`
- **Cursor rules:** `.cursor/rules/duplication.mdc`
- **Before committing:** Run `make dup` to check for duplicates
