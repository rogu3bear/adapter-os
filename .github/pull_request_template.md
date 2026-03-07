## Summary

Describe what this PR changes and why.

## Linked Work

- [ ] Linked issue / blocker: `#`
- [ ] This branch is based on `origin/main` (or the divergence is explicitly documented below)

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
- [ ] Updated relevant documentation if changing load/route/run/record/replay flows
- [ ] Added inline code comments for complex logic (prefer `///` doc comments for public APIs)
- [ ] Updated ARCHITECTURE.md or docs/ if changing core patterns or subsystems

### Governance Impact
- [ ] Reviewed governance impact (paths, generated artifacts, tooling state)
- [ ] If generated artifacts changed, documented generator command and validation check
- [ ] No unauthorized tracked files under local-only tooling/runtime paths

### GitHub Hygiene
- [ ] Labels applied
- [ ] Assignee and review request set
- [ ] Expected failing checks documented below (or `none`)
- [ ] Rollback plan documented below

### Database Optimizations (Required when changing DB performance)
- [ ] If this PR includes a DB optimization (indexes, query rewrites, PRAGMA tuning, ANALYZE jobs):
  - [ ] Added/updated an entry in [`optimizations/db/registry.toml`](optimizations/db/registry.toml:1)
  - [ ] Declared `touches` + resolved conflicts/dependencies before implementation
  - [ ] Documented `canary` + `rollback` procedure and provided rollback script(s) when applicable
  - [ ] Attached impact assessment (baseline EXPLAIN + expected/observed p95 deltas)

## Verification

List the exact commands you ran:

```bash
```

## Expected Failing Checks

State `none` or list the exact check names with justification.

## Rollback Plan

State the revert path if this lands and regresses.

## Notes

Add any migration notes, breaking changes, or follow-ups.
