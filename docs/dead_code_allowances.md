# Dead Code Allowances Audit (snapshot: 2026-01-25)

> **Snapshot** — Re-run `rg "dead_code"` for current counts. See [DOCS_AUDIT_2026-02-18.md](DOCS_AUDIT_2026-02-18.md).

Purpose: track, prune, and prevent dead-code drift. Source data taken via `rg "dead_code"` on 2026-01-25.

## Stop Condition
- All production crates free of unnecessary dead-code allowances (crate-level + item-level).
- Tests/benches reviewed individually; outdated cases removed or replaced.
- Coverage re-run to confirm no new dead-code lint suppressions introduced.

## Current Inventory (counts)
- Total markers: 304
- Blanket crate/file disables (`#![allow(dead_code)]`): 46
- By area: crates (142 files), tests (18), training data (3), examples (2), xtask (1)
- Top-crate density: adapteros-server-api (22), adapteros-db (19), adapteros-lora-worker (17), adapteros-lora-mlx-ffi (10), adapteros-cli (10), adapteros-ui (8), adapteros-tui (6), adapteros-codegraph (6)

## Highest-Risk Blanket Allows (production)
- ~~crates/adapteros-policy/src/lib.rs~~ (blanket removed 2026-01-25)
- ~~crates/adapteros-secd/src/lib.rs~~ (blanket removed 2026-01-25)
- ~~crates/adapteros-deterministic-exec/src/lib.rs~~ (blanket removed 2026-01-25)
- ~~crates/adapteros-memory/src/lib.rs~~ (blanket removed 2026-01-25)
- ~~crates/adapteros-lora-worker/src/lib.rs~~ (blanket removed 2026-01-25)
- ~~crates/adapteros-base-llm/src/lib.rs~~ (blanket removed 2026-01-25)
- ~~crates/adapteros-lora-kernel-coreml/src/lib.rs~~ (blanket removed 2026-01-25)
- ~~crates/adapteros-cli/src/main.rs~~ (blanket removed 2026-01-25)
- crates/adapteros-server/src/alerting.rs (converted to `#![allow(unused)]` pending removal)

## File Hotspots (high item counts)
- crates/adapteros-memory/src/page_migration_iokit.rs (17)
- crates/adapteros-cli/src/commands/config.rs (16)
- crates/adapteros-tui/src/app/api.rs (12)
- crates/adapteros-lora-worker/src/training/trainer.rs (11)
- crates/adapteros-crypto/src/providers/keychain.rs (7)
- crates/adapteros-server-api/tests/support/e2e_harness.rs (7)
- crates/adapteros-lora-worker/benches/mlx_bridge_streaming.rs (6)
- crates/adapteros-lora-mlx-ffi/src/embedding.rs (5)
- crates/adapteros-db/src/kv_isolation_scan.rs (4)
- crates/adapteros-lora-kernel-mtl/src/ane_acceleration.rs (4)

## Triage Plan (prod first)
1) Remove crate-level `#![allow(dead_code)]` in the list above; replace with targeted item-level allows only where a future-facing variant truly needs to stay. Document rationale inline.
2) Sweep hotspot files: delete unused items or gate behind feature flags; keep minimal `#[allow(dead_code)]` with intent comments.
3) Run `cargo clippy --workspace -- -D dead_code` after each crate sweep to surface remaining unused symbols.
4) Only after production sweep is clean, review test/bench harnesses one by one; drop obsolete suites or modernize where valuable.
5) Strip non-code noise (3 markers in training data JSON, 2 in examples) to keep counts signal-strong.

## Tracking Rules
- When keeping an allowance, add an inline comment with owner + reason + expected removal trigger.
- Update this file with date-stamped snapshots after each sweep section completes.
- Prevent regressions by adding a CI lint stage or a local pre-push check: `cargo clippy --workspace -- -D dead_code` (can be temporarily scoped with `-p <crate>` during cleanup).
