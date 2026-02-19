# Stable Mainline Unification (2026-02-17)

> **Archived snapshot** — Historical merge record. Code in `crates/adapteros-ui/` is authoritative.

## Goal
- Deterministically unify the UI partial-feature stream into a `main`-based stable branch.
- Explicitly remove duplicate exports.
- Reconcile dependency state with exact commit and guideline citations.

## Codebase Guideline Citations
- Existing-code-first rule: `AGENTS.md:6` and `AGENTS.md:8`.
- Minimal diff / avoid refactor: `AGENTS.md:13`.
- Smallest relevant verification commands: `AGENTS.md:12`, `AGENTS.md:21`, `AGENTS.md:37`, `AGENTS.md:48`.

## Integration Baseline
- Target base branch: `main` at `073d932b62f0d52c1122576ce0ecb84967e2f3b6`.
- Integration branch: `c/stable-main-unify`.
- Source partial-feature branch: `c/ui-functional-dedup-sprint`.

## Deterministic Commit Unification
1. Cherry-picked `8166b9fad` -> `b969c5548`.
2. Cherry-picked `6b607eab4` -> `40d8f5553`.
3. Cherry-picked `b9692fafa` -> `022fed718`.
4. Duplicate-export cleanup commit -> `86f89da7e`.

No cherry-pick conflicts occurred; all commits applied cleanly in topological order.

## Duplicate Export Removal (Explicit)
- Duplicate identified: `layout::BreadcrumbItem` exported twice in `crates/adapteros-ui/src/components/mod.rs` (as both `BreadcrumbItem` and `PageBreadcrumbItem`).
- Deterministic resolution: keep `PageBreadcrumbItem` (active usage across UI pages) and remove redundant `BreadcrumbItem` root export.
- Final code citation: `crates/adapteros-ui/src/components/mod.rs:97`.
- Cleanup commit: `86f89da7ecde3edb463b26d738e146ff9999680a`.

## Dependency Reconciliation (Deterministic)
- Checked for manifest drift between `main` and unified branch:
  - `git diff --name-only main..HEAD | rg 'Cargo.toml|Cargo.lock'`
  - Result: no dependency manifest or lockfile changes.
- Dependency baseline retained in `crates/adapteros-ui/Cargo.toml:18` through `crates/adapteros-ui/Cargo.toml:89`.
- Reconciliation outcome: dependency graph inputs unchanged; integration was code-only plus export dedupe.

## Verification Evidence
- Compile check (pass):
  - `cargo check -p adapteros-ui --target wasm32-unknown-unknown`
- Similarity gate (pass):
  - `python3 scripts/ui_component_similarity.py --threshold 0.80 --exclude-file-suffix components/icons.rs --max-qualifying 8`
  - Result: `Qualifying components: 6`, `Max qualifying: 8 (PASS)`.
- `trunk test` status:
  - Command unavailable in environment (`trunk` has no `test` subcommand).
- `cargo test -p adapteros-ui` status:
  - compile progressed but runner stalled; compile verification already covered by `cargo check`.

## Final Commit Chain (Newest First)
- `86f89da7ecde3edb463b26d738e146ff9999680a` `ui: remove duplicate BreadcrumbItem export`
- `022fed718c9bcd8e03f0908fcb3d8829ce366e74` `test(ci): harden ui component similarity parser`
- `40d8f5553f154f2f16bc10974156d916deea0d8d` `test(ci): align ui wasm imports and add similarity guard`
- `b969c5548f5699f30f4ff909f660088bd1ad7851` `ui: integrate cross-surface cleanup and navigation clarity`
- Base: `073d932b62f0d52c1122576ce0ecb84967e2f3b6`
