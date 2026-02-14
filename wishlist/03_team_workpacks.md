# Team Workpacks (Sequential)

## Workpack Format
- `Objective`
- `Owned paths`
- `No-duplication guardrails`
- `Execution checklist`
- `Verification`

## WP-3A: Determinism Seed + Ledger Contract (Phase 3)
- Objective: unify seed and tick-ledger semantics without cross-crate duplication.
- Owned paths:
  - `crates/adapteros-core/src/seed.rs`
  - `crates/adapteros-core/src/determinism.rs`
  - `crates/adapteros-deterministic-exec/src/seed.rs`
  - `crates/adapteros-deterministic-exec/src/global_ledger.rs`
- No-duplication guardrails:
  - Extend existing seed context types before creating any new context object.
  - Reuse existing telemetry event enums where possible.
- Execution checklist:
  - [ ] Build side-by-side seed lifecycle map.
  - [ ] Identify shared invariants and drift points.
  - [ ] Define one bridge strategy (core authoritative, exec adapter layer).
  - [ ] Draft targeted tests to prove non-divergence.
- Verification:
  - `cargo test -p adapteros-lora-router --test determinism`
  - `cargo test --test determinism_core_suite`

## WP-4B: Control Plane Config + Readiness Convergence (Phase 4)
- Objective: eliminate config/invariant drift between server boot and API readiness.
- Owned paths:
  - `crates/adapteros-server/src/config.rs`
  - `crates/adapteros-server-api/src/config.rs`
  - `crates/adapteros-server-api/src/handlers/health.rs`
- No-duplication guardrails:
  - Reuse shared config schema definitions; avoid introducing a third wrapper.
  - Keep boot/readiness invariants generated from one mapping table.
- Execution checklist:
  - [ ] Build config field parity matrix.
  - [ ] Mark source-of-truth owner per field group.
  - [ ] Build boot-state to ready-state mapping table.
  - [ ] Draft single-point invariant utility proposal.
- Verification:
  - `cargo test -p adapteros-server-api`

## WP-5C: UI Route-Action Parity (Phase 5)
- Objective: remove route/action metadata drift in UI navigation + command palette.
- Owned paths:
  - `crates/adapteros-ui/src/components/layout/nav_registry.rs`
  - `crates/adapteros-ui/src/search/contextual.rs`
  - `crates/adapteros-ui/src/search/index.rs`
- No-duplication guardrails:
  - Pull contextual behavior from nav metadata; do not add extra route registries.
- Execution checklist:
  - [ ] Build route literal inventory from all three files.
  - [ ] Identify mismatches/missing ownership.
  - [ ] Define shared metadata extraction strategy.
  - [ ] Propose migration order with smallest diffs first.
- Verification:
  - `cargo check -p adapteros-ui`

## WP-6CD: Static Diagnostics Harmonization (Phase 6)
- Objective: keep static and static-minimal boot diagnostics behavior aligned.
- Owned paths:
  - `crates/adapteros-server/static/index.html`
  - `crates/adapteros-server/static-minimal/index-minimal.html`
  - `crates/adapteros-server/static-minimal/api-test.html`
- No-duplication guardrails:
  - Shared snippet/include pattern only; no separate duplicated JS blocks.
- Execution checklist:
  - [ ] Diff diagnostic scripts/handlers across files.
  - [ ] Define common snippet boundaries.
  - [ ] Map migration path preserving current behavior.
- Verification:
  - `scripts/ci/check_ui_assets.sh`

## WP-7D: Docs Contract Consolidation (Phase 7)
- Objective: single docs-claims gate with canonical artifact generation flow.
- Owned paths:
  - `scripts/contracts/generate_contract_artifacts.py`
  - `scripts/contracts/check_docs_claims.sh`
  - `scripts/validate-docs.sh`
  - `docs/CANONICAL_SOURCES.md`
- No-duplication guardrails:
  - Extend contract checks before adding any new docs scan script.
- Execution checklist:
  - [ ] Classify overlap between contract checks and legacy validator checks.
  - [ ] Decide keep/merge/deprecate per check.
  - [ ] Define migration and CI gating order.
- Verification:
  - `scripts/contracts/check_docs_claims.sh`

## WP-8AB: Security + Replay Evidence Closure (Phase 8)
- Objective: close claim gaps for replay, determinism, and tenant boundaries.
- Owned paths:
  - `docs/VERIFIED_REPO_FACTS.md`
  - `docs/replay.md`
  - `crates/adapteros-server-api/src/routes/mod.rs`
- No-duplication guardrails:
  - Keep one canonical replay family and explicit deprecation flow.
- Execution checklist:
  - [ ] Re-verify replay family claims from source files.
  - [ ] Mark canonical endpoints and deprecation candidates.
  - [ ] Update evidence table with date and source anchors.
- Verification:
  - `rg -n "replay" crates/adapteros-server-api/src/routes/mod.rs docs/replay.md`

## WP-9E: Integration Sheriff Closeout (Phase 9)
- Objective: integrate all outputs and clear residual cross-team conflicts.
- Owned paths:
  - `wishlist/*.md`
  - `docs/generated/*.json`
  - `scripts/ci/stability.sh`
- No-duplication guardrails:
  - No merging of conflicting proposals without explicit owner arbitration.
- Execution checklist:
  - [ ] Build conflict matrix across workpacks.
  - [ ] Finalize verification matrix.
  - [ ] Publish release-readiness memo and residual risks.
- Verification:
  - `scripts/ci/stability.sh`

