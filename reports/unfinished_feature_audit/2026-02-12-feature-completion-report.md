# Incomplete Feature Completion Report

- generated_at_utc: 2026-02-12T06:00:12Z
- execution_branch: c/finish-incomplete-features-20260212
- baseline_reconciliation_commit: 0a02bab3d3f1518994677f3f44f08a60df4a894c
- deterministic_order: lexical by UF id
- targeted_features: 7
- completed: 6
- scoped_complete_with_residual_limitations: 1

## Completion Decisions
| Order | UF | Prior staging tip | Completion commit | Decision | Primary citations |
|---|---|---|---|---|---|
| 01 | UF-02 | e1c795df67184eec6a89f617789cd25bb01aab87 | d415d7c88c196b9c786e78936ca3bd2970577276 | COMPLETED | `crates/adapteros-server-api/src/event_bus.rs:21` `crates/adapteros-server-api/src/event_bus.rs:49` |
| 02 | UF-06 | 6ad32f396036663a83dbd44f998caa91f9065ae9 | d6d2eaddc901a9a369c394cc020be5df011297df | COMPLETED | `crates/adapteros-crypto/src/providers/kms.rs:1933` `crates/adapteros-crypto/src/providers/kms.rs:2062` |
| 03 | UF-08 | 475accf296f190aaef1731ba1a32d04e88b3ebcd | 8dbe7d344d6ea1420fadc861c31bbed5b11be4d1 | COMPLETED | `crates/adapteros-memory/src/pressure_manager.rs:136` `crates/adapteros-memory/src/pressure_manager.rs:152` |
| 04 | UF-10 | 3020b290169a7da89f2fa453101886dae1c42a48 | 979c00c383d32fef2ab520c5403fc0e89fe0ec5a | COMPLETED | `crates/adapteros-telemetry/src/metrics/system.rs:98` `crates/adapteros-telemetry/src/metrics/system.rs:106` |
| 05 | UF-12 | 6b5fdb1c929081dc64c388a8fa4d143d5d9f3d03 | 4ed447f0f93c628b2bb896b307bf0796cadb6494 | COMPLETED | `crates/adapteros-api-types/src/ui.rs:53` `crates/adapteros-server-api/src/handlers/ui_config.rs:29` `crates/adapteros-ui/src/signals/ui_profile.rs:37` `crates/adapteros-ui/src/constants.rs:58` `crates/adapteros-ui/src/components/layout/topbar.rs:34` |
| 06 | UF-13 | bcce785983fccd319589c1bfac12996e6a09ea13 | c4a2fd11b0d193989bf30b21d9639c80bbd56538 | COMPLETED | `crates/adapteros-ui/src/api/mod.rs:100` `crates/adapteros-ui/src/pages/settings/api_config.rs:223` `crates/adapteros-ui/src/components/chat_dock.rs:518` `crates/adapteros-ui/src/pages/chat.rs:778` `crates/adapteros-ui/src/signals/settings.rs:110` |
| 07 | UF-16 | 0fd4e8ab3de1bcdaee2693797d1deb524a55c376 | 607595a72ee0741e45281a7fc382615e09fdccb5 | SCOPED_COMPLETE_WITH_RESIDUAL_LIMITATIONS | `crates/adapteros-federation/src/lib.rs:146` `crates/adapteros-federation/src/lib.rs:1098` `docs/TECHNICAL_SPECIFICATION.md:2271` `docs/TECHNICAL_SPECIFICATION.md:2274` |

## Verification Matrix
| UF | Command | Result |
|---|---|---|
| UF-02 | `cargo test -p adapteros-server-api --lib event_bus::tests::test_event_bus_basic_dispatch -- --nocapture` | PASS |
| UF-06 | `cargo test -p adapteros-crypto test_kms_provider_attest -- --nocapture` | PASS |
| UF-08 | `cargo test -p adapteros-memory pressure_manager::tests::test_pressure_manager_creation -- --nocapture` | PASS |
| UF-10 | `cargo test -p adapteros-telemetry metrics::system::tests::test_system_metrics_event_serializes -- --nocapture` | PASS |
| UF-12 | `cargo check -p adapteros-api-types -p adapteros-server-api -p adapteros-ui` | PASS |
| UF-13 | `cargo check -p adapteros-ui` | PASS |
| UF-16 | `AOS_SKIP_MIGRATION_SIGNATURES=1 cargo test -p adapteros-federation test_get_latest_tick_hash_reads_latest_tenant_tick -- --nocapture` | PASS |

## Notes
- UF-16 removed the tick-linkage stub and added a test for latest-tenant tick lookup, but the spec still correctly documents that cross-host federation hardening and PostgreSQL multi-node deployment are not yet complete (`docs/TECHNICAL_SPECIFICATION.md:2271`, `docs/TECHNICAL_SPECIFICATION.md:2274`).
- Machine-readable ledger: `reports/unfinished_feature_audit/2026-02-12-feature-completion-ledger.tsv`.
