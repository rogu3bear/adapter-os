## Summary

Describe what this PR changes and why.

## Changes

- [ ] Feature / Fix 1
- [ ] Feature / Fix 2

## Checklist

### Code Quality
- [ ] Builds with `cargo check --workspace --all-targets`
- [ ] Passes formatting with `cargo fmt --all -- --check`
- [ ] (Advisory) Clippy reviewed: `cargo clippy --workspace --all-targets`
- [ ] Added/updated tests where appropriate
- [ ] Duplication scan reviewed (see Duplication Scan action and `docs/DUPLICATION_MONITORING.md`)

### Documentation (Required for significant changes)
- [ ] Updated relevant flow documentation if changing load/route/run/record/replay flows (see `docs/flows/`)
- [ ] Updated `docs/flows/diagrams.md` if adding new telemetry events or changing state machines
- [ ] Updated `docs/architecture.md` Reality vs Plan section if changing implementation status
- [ ] Added inline code comments for complex logic (prefer `///` doc comments for public APIs)
- [ ] Updated AGENTS.md if changing core patterns, architecture, or adding new subsystems

### Database Optimizations (Required when changing DB performance)
- [ ] If this PR includes a DB optimization (indexes, query rewrites, PRAGMA tuning, ANALYZE jobs):
  - [ ] Added/updated an entry in [`optimizations/db/registry.toml`](optimizations/db/registry.toml:1)
  - [ ] Declared `touches` + resolved conflicts/dependencies before implementation
  - [ ] Documented `canary` + `rollback` procedure and provided rollback script(s) when applicable
  - [ ] Attached impact assessment (baseline EXPLAIN + expected/observed p95 deltas)

**Documentation Guidelines**:
- New telemetry events → Add to `docs/flows/diagrams.md` § 5. Telemetry Event Schema
- New state transitions → Update Mermaid diagram in `docs/flows/diagrams.md` § 2. Lifecycle State Machine
- New flows/patterns → Create new `docs/flows/{name}.md` following existing template
- Implementation status changes → Update `docs/architecture.md` Reality vs Plan tables

## Notes

Add any migration notes, breaking changes, or follow-ups.
