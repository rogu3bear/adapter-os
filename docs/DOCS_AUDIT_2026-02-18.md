# Documentation Audit (2026-02-18)

**Purpose:** Categorize docs by staleness risk and temp status. Code is authoritative.

## Temp / Snapshot Docs (Re-verify before trusting)

These docs capture point-in-time snapshots. Numbers, status, and structure may have changed.

| Doc | Type | Risk |
|-----|------|------|
| `TEST_RESULTS_AFTER_ALL_TESTS.md` | Benchmark results (2026-01-31) | Hardware/code changes invalidate numbers |
| `program/METRICS.md` | Program phase verification | Phase status may have moved |
| `program/EXECUTION_PLAN.md` | Epic status tracking | PRs may have merged |

## Snapshot / Audit Docs (Re-run for current state)

| Doc | Type | Risk |
|-----|------|------|
| `dead_code_allowances.md` | Dead-code audit (2026-01-25) | Counts drift as code changes |
| `VERIFIED_REPO_FACTS.md` | Audit snapshot | Re-verify against current code |
| `audits/unfinished_feature_isolation_2026-02-17.md` | Feature isolation scan | Branches/units may have changed |
| `reconciliation/*` | Merge records | Historical only |

## Benchmark / Results Docs (Numbers may change)

| Doc | Type | Risk |
|-----|------|------|
| `MODEL_SERVER_BENCHMARKS.md` | Performance metrics | Hardware/model changes |
| `performance/K_SPARSE_ROUTER_BASELINE.md` | Router baseline | Code changes |

## Reference Docs (Canonical source in code)

| Doc | Canonical Source | Notes |
|-----|------------------|-------|
| `TRAINING_METRICS.md` | `crates/adapteros-lora-worker/src/training/metrics.rs` | Verify module paths |
| `TRAINING_PIPELINE.md` | `crates/adapteros-lora-worker/`, orchestrator | Pipeline phases |
| `training/REPORTING.md` | `var/artifacts/training-reports/` | Report schema |
| `DATASET_TEST_SUITE.md` | Dataset validation code | Schema tiers |
| `DETERMINISM_PATCH_INVENTORY.md` | Patch series | May be applied; verify |
| `DETERMINISM_REGRESSION.md` | `adapteros-core`, `adapteros-db` tests | Harness commands |

## Dropped (2026-02-18)

- `reconciliation/partial_branch_reconciliation_2026-02-17.json`
- `reconciliation/partial_branch_deletions_2026-02-17.json`
- `reconciliation/partial_branch_reconciliation_after_merge_2026-02-17.json`
- `audits/unfinished_feature_isolation_2026-02-17.json`
- `engineering/ROUTE_MAP.md` (redundant; `api/ROUTE_MAP.md` is canonical)

## Plans (Historical; code is authoritative)

All 17 files in `plans/` — see [plans/README.md](plans/README.md).

## Runbooks (Operational; verify commands)

Runbooks in `runbooks/` are operational guides. Commands and paths may have changed. Verify before use.

## Agent Staleness Headers (2026-02-18)

The following docs now include an **Agent note** blockquote instructing agents to re-verify before trusting:

- ARCHITECTURE.md, BOOT_WALKTHROUGH.md, BOOT_PHASES.md, BACKEND_ARCHITECTURE.md
- UI_WALKTHROUGH.md, API_GUIDES.md, CLI_GUIDE.md, MLX_GUIDE.md, VISUAL_GUIDES.md
- getting-started.md, AUTHENTICATION.md, MODEL_MANAGEMENT.md, DATASET_TEST_SUITE.md
- TROUBLESHOOTING_INDEX.md, TROUBLESHOOTING_ENHANCED.md, BOOT_TROUBLESHOOTING.md
- ENDPOINTS_TRUTH_TABLE.md, DATABASE.md, OPERATIONS.md, TRAINING.md
- CANONICAL_USER_WORKFLOW.md, ui/data-flow.md, runbooks/README.md

Each points to [CANONICAL_SOURCES.md](CANONICAL_SOURCES.md) and this audit.

## Policy

1. If a doc claims "X exists" or "Y is at path Z", verify in code before trusting.
2. Benchmark/result numbers are snapshots; re-run for current accuracy.
3. Program/epic status docs are progress trackers; check PRs for actual state.
