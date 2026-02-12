# Partial Branch Reconciliation

- generated_at_utc: 2026-02-12T05:26:59Z
- reconciliation_branch: main
- reconciliation_head_start: 44e1c1dfc7cecc88c9590dcdff61fbcef09b2677
- deterministic_order: lexical by UF id
- partial_branches_considered: 7
- completed_features_merged: 0

## Decision Table
| Order | UF | Branch | Branch tip | Status | Action | Merge commit | Evidence |
|---|---|---|---|---|---|---|---|
| 01 | UF-02 | c/stage/uf-02-event-bus-doc-example-todo | e1c795df67184eec6a89f617789cd25bb01aab87 | INCOMPLETE | SKIPPED_NOT_COMPLETE | - | crates/adapteros-server-api/src/event_bus.rs:21@b15371f220d9d5f974d3c163b450424b3e1a7112 |
| 02 | UF-06 | c/stage/uf-06-crypto-kms-attestation-placeholder-signature | 6ad32f396036663a83dbd44f998caa91f9065ae9 | INCOMPLETE | SKIPPED_NOT_COMPLETE | - | crates/adapteros-crypto/src/providers/kms.rs:1932@b15371f220d9d5f974d3c163b450424b3e1a7112 |
| 03 | UF-08 | c/stage/uf-08-memory-pressure-current-k-placeholder | 475accf296f190aaef1731ba1a32d04e88b3ebcd | INCOMPLETE | SKIPPED_NOT_COMPLETE | - | crates/adapteros-memory/src/pressure_manager.rs:137@b15371f220d9d5f974d3c163b450424b3e1a7112 |
| 04 | UF-10 | c/stage/uf-10-telemetry-disk-io-placeholder | 3020b290169a7da89f2fa453101886dae1c42a48 | INCOMPLETE | SKIPPED_NOT_COMPLETE | - | crates/adapteros-telemetry/src/metrics/system.rs:97@f34791f712e5ef324af524aacf05e5a4f27aca71 |
| 05 | UF-12 | c/stage/uf-12-ui-docs-link-runtime-config-todo | 6b5fdb1c929081dc64c388a8fa4d143d5d9f3d03 | INCOMPLETE | SKIPPED_NOT_COMPLETE | - | crates/adapteros-ui/src/constants.rs:48@6311742615d6aea2e8408c712409d5ecc3d67ede |
| 06 | UF-13 | c/stage/uf-13-ui-settings-toggles-unconsumed | bcce785983fccd319589c1bfac12996e6a09ea13 | INCOMPLETE | SKIPPED_NOT_COMPLETE | - | crates/adapteros-ui/src/signals/settings.rs:105@265cf1017882c8b7578f5fc49696c336295b5877 |
| 07 | UF-16 | c/stage/uf-16-docs-federation-postgres-partial | 0fd4e8ab3de1bcdaee2693797d1deb524a55c376 | INCOMPLETE | SKIPPED_NOT_COMPLETE | - | docs/TECHNICAL_SPECIFICATION.md:2270@31306828db997ce3d7ae40e9de1f8ef4d466cd3f |

## Result
No partial branches met completion criteria; no feature branches were merged in this reconciliation pass.

## Artifact
- reports/unfinished_feature_audit/2026-02-12-partial-merge-ledger.tsv
