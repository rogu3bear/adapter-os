# Unfinished / Speculative / Partial Feature Audit

- generated_at_utc: 2026-02-12T04:28:11Z
- scan_branch: main
- scan_head_commit: 5f5f60d642db551395037fe13aba0bb2405dcd4d
- deterministic_isolation_base_commit: 5f5f60d642db551395037fe13aba0bb2405dcd4d
- textual_marker_hits: 2044
- todo_unimplemented_macro_hits: 24

## Scope And Method
- Scope: full repository scan excluding .git, target, and node_modules directories.
- Signal classes: unfinished, speculative, partial.
- Agent team partitions: Runtime/Backends, Data/Training/Security, UI, Scripts/Docs/Ops, Tests/Examples.
- Deterministic isolation rule: one branch per UF bucket, all branches forked from the same base commit, each with a single evidence-note commit.

## Findings + Isolation Mapping
| UF | Class | Finding | Evidence refs (file:line@commit) | Staging branch | Staging commit |
|---|---|---|---|---|---|
| UF-01 | unfinished | Boot invariants config helper remains unimplemented | crates/adapteros-server/tests/boot_invariants_tests.rs:33@c3fd527e8e8bad5eec62db1e549d1b080905bcab | c/stage/uf-01-boot-invariants-test-harness | 0e7bbcffeb42c259ac3f846d4c82c3d244373f44 |
| UF-02 | partial | Server API event bus example still uses todo macro | crates/adapteros-server-api/src/event_bus.rs:21@b15371f220d9d5f974d3c163b450424b3e1a7112 | c/stage/uf-02-event-bus-doc-example-todo | e1c795df67184eec6a89f617789cd25bb01aab87 |
| UF-03 | unfinished | Server API quickstart AppState constructor is todo | crates/adapteros-server-api/src/services/QUICKSTART.md:269@b15371f220d9d5f974d3c163b450424b3e1a7112 | c/stage/uf-03-server-api-quickstart-appstate-todo | e1b328a4e6f37b4ccddee560b7959a6afd37046d |
| UF-04 | unfinished | Linux keychain deletion path is a stub implementation | crates/adapteros-crypto/src/providers/keychain.rs:2302@b15371f220d9d5f974d3c163b450424b3e1a7112 | c/stage/uf-04-crypto-linux-keychain-delete-stub | 0ca6eaa93e776e6a6406b62908db68e914e3e276 |
| UF-05 | speculative | GCP KMS provider is emulator/mock stub | crates/adapteros-crypto/src/providers/gcp.rs:33@fe8005e2b732f0a7f662826840fe804e5050434c | c/stage/uf-05-crypto-gcp-kms-emulator-stub | 566bb9fce5197d3e0fbbb45eebe595d0f5313997 |
| UF-06 | partial | KMS attestation uses placeholder signature bytes | crates/adapteros-crypto/src/providers/kms.rs:1932@b15371f220d9d5f974d3c163b450424b3e1a7112 | c/stage/uf-06-crypto-kms-attestation-placeholder-signature | 6ad32f396036663a83dbd44f998caa91f9065ae9 |
| UF-07 | speculative | Dev signature bypass returns placeholder bundle signature | crates/adapteros-crypto/src/bundle_sign.rs:206@b15371f220d9d5f974d3c163b450424b3e1a7112 | c/stage/uf-07-crypto-dev-signature-bypass-placeholder | 9e181c5ff07df32239418ab3a1e7f69c238d553b |
| UF-08 | partial | Pressure manager hard-codes placeholder current K | crates/adapteros-memory/src/pressure_manager.rs:137@b15371f220d9d5f974d3c163b450424b3e1a7112 | c/stage/uf-08-memory-pressure-current-k-placeholder | 475accf296f190aaef1731ba1a32d04e88b3ebcd |
| UF-09 | unfinished | IOKit memory pressure callback registration is placeholder | crates/adapteros-memory/src/page_migration_iokit_impl.mm:257@b15371f220d9d5f974d3c163b450424b3e1a7112 | c/stage/uf-09-memory-iokit-callback-placeholder | 86db0a78f407f4ec1c7e9242375f3a59d29f712a |
| UF-10 | partial | Telemetry disk I/O counters return placeholder zeros | crates/adapteros-telemetry/src/metrics/system.rs:96@f34791f712e5ef324af524aacf05e5a4f27aca71 | c/stage/uf-10-telemetry-disk-io-placeholder | 3020b290169a7da89f2fa453101886dae1c42a48 |
| UF-11 | unfinished | Dependency security policy falls back to stubbed offline DB | crates/adapteros-policy/src/packs/dependency_security.rs:690@b15371f220d9d5f974d3c163b450424b3e1a7112 | c/stage/uf-11-policy-dependency-security-offline-stub | c3901d8904bd3509094b965cbb29d41e1ce894c0 |
| UF-12 | partial | UI docs link should be runtime configurable but is TODO | crates/adapteros-ui/src/constants.rs:48@6311742615d6aea2e8408c712409d5ecc3d67ede | c/stage/uf-12-ui-docs-link-runtime-config-todo | 6b5fdb1c929081dc64c388a8fa4d143d5d9f3d03 |
| UF-13 | partial | UI persisted settings toggles are not yet consumed | crates/adapteros-ui/src/signals/settings.rs:110@265cf1017882c8b7578f5fc49696c336295b5877<br>crates/adapteros-ui/src/signals/settings.rs:116@5a2b3fffb34e18a55da424f933b352b11c295a5b | c/stage/uf-13-ui-settings-toggles-unconsumed | bcce785983fccd319589c1bfac12996e6a09ea13 |
| UF-14 | unfinished | Smoke test lacks non-SQLite connectivity probe | scripts/test/all.sh:100@b15371f220d9d5f974d3c163b450424b3e1a7112 | c/stage/uf-14-scripts-non-sqlite-probe-not-implemented | 79ed31788159b3faabe2798ad66d971fd252032a |
| UF-15 | speculative | Worker debug bypass path expects placeholder sleep process | scripts/test/test_worker_fix.sh:18@b15371f220d9d5f974d3c163b450424b3e1a7112 | c/stage/uf-15-scripts-worker-placeholder-bypass-path | 4b1c5abe880b050c358b12c11a9b659c7e17281b |
| UF-16 | partial | Tech spec documents federation and PostgreSQL multi-node as partial | docs/TECHNICAL_SPECIFICATION.md:2266@31306828db997ce3d7ae40e9de1f8ef4d466cd3f | c/stage/uf-16-docs-federation-postgres-partial | 0fd4e8ab3de1bcdaee2693797d1deb524a55c376 |
| UF-17 | unfinished | Policy enforcement integration tests are ignored pending API updates | tests/policy_enforcement_integration.rs:1@2c706b04ec52604a17e15ca898415f7dbc565080 | c/stage/uf-17-tests-policy-enforcement-ignored | 0285ee1812bfeffd93eeed2ca3bdc9895c55ccaf |
| UF-18 | unfinished | GPU backend selection/discovery integration tests ignored | crates/adapteros-lora-worker/tests/gpu_training_integration.rs:76@2c706b04ec52604a17e15ca898415f7dbc565080<br>crates/adapteros-lora-worker/tests/gpu_training_integration.rs:155@2c706b04ec52604a17e15ca898415f7dbc565080 | c/stage/uf-18-tests-gpu-backend-coverage-ignored | f54edc079650df0d0876fb7ea1fc799dbf579b2e |
| UF-19 | unfinished | MLX memory management integration test file is stubbed | crates/adapteros-lora-mlx-ffi/tests/memory_management_integration.rs:1@2c706b04ec52604a17e15ca898415f7dbc565080 | c/stage/uf-19-tests-mlx-memory-management-stubbed | 08c23e8afca2e90e7ece7b67211bcc4c5f60cc9e |

## Move Ledger
| UF | Move (base -> branch) | Note commit | Note path |
|---|---|---|---|
| UF-01 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-01-boot-invariants-test-harness | 0e7bbcffeb42c259ac3f846d4c82c3d244373f44 | reports/unfinished_feature_audit/staging/UF-01-boot-invariants-test-harness.md |
| UF-02 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-02-event-bus-doc-example-todo | e1c795df67184eec6a89f617789cd25bb01aab87 | reports/unfinished_feature_audit/staging/UF-02-event-bus-doc-example-todo.md |
| UF-03 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-03-server-api-quickstart-appstate-todo | e1b328a4e6f37b4ccddee560b7959a6afd37046d | reports/unfinished_feature_audit/staging/UF-03-server-api-quickstart-appstate-todo.md |
| UF-04 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-04-crypto-linux-keychain-delete-stub | 0ca6eaa93e776e6a6406b62908db68e914e3e276 | reports/unfinished_feature_audit/staging/UF-04-crypto-linux-keychain-delete-stub.md |
| UF-05 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-05-crypto-gcp-kms-emulator-stub | 566bb9fce5197d3e0fbbb45eebe595d0f5313997 | reports/unfinished_feature_audit/staging/UF-05-crypto-gcp-kms-emulator-stub.md |
| UF-06 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-06-crypto-kms-attestation-placeholder-signature | 6ad32f396036663a83dbd44f998caa91f9065ae9 | reports/unfinished_feature_audit/staging/UF-06-crypto-kms-attestation-placeholder-signature.md |
| UF-07 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-07-crypto-dev-signature-bypass-placeholder | 9e181c5ff07df32239418ab3a1e7f69c238d553b | reports/unfinished_feature_audit/staging/UF-07-crypto-dev-signature-bypass-placeholder.md |
| UF-08 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-08-memory-pressure-current-k-placeholder | 475accf296f190aaef1731ba1a32d04e88b3ebcd | reports/unfinished_feature_audit/staging/UF-08-memory-pressure-current-k-placeholder.md |
| UF-09 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-09-memory-iokit-callback-placeholder | 86db0a78f407f4ec1c7e9242375f3a59d29f712a | reports/unfinished_feature_audit/staging/UF-09-memory-iokit-callback-placeholder.md |
| UF-10 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-10-telemetry-disk-io-placeholder | 3020b290169a7da89f2fa453101886dae1c42a48 | reports/unfinished_feature_audit/staging/UF-10-telemetry-disk-io-placeholder.md |
| UF-11 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-11-policy-dependency-security-offline-stub | c3901d8904bd3509094b965cbb29d41e1ce894c0 | reports/unfinished_feature_audit/staging/UF-11-policy-dependency-security-offline-stub.md |
| UF-12 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-12-ui-docs-link-runtime-config-todo | 6b5fdb1c929081dc64c388a8fa4d143d5d9f3d03 | reports/unfinished_feature_audit/staging/UF-12-ui-docs-link-runtime-config-todo.md |
| UF-13 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-13-ui-settings-toggles-unconsumed | bcce785983fccd319589c1bfac12996e6a09ea13 | reports/unfinished_feature_audit/staging/UF-13-ui-settings-toggles-unconsumed.md |
| UF-14 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-14-scripts-non-sqlite-probe-not-implemented | 79ed31788159b3faabe2798ad66d971fd252032a | reports/unfinished_feature_audit/staging/UF-14-scripts-non-sqlite-probe-not-implemented.md |
| UF-15 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-15-scripts-worker-placeholder-bypass-path | 4b1c5abe880b050c358b12c11a9b659c7e17281b | reports/unfinished_feature_audit/staging/UF-15-scripts-worker-placeholder-bypass-path.md |
| UF-16 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-16-docs-federation-postgres-partial | 0fd4e8ab3de1bcdaee2693797d1deb524a55c376 | reports/unfinished_feature_audit/staging/UF-16-docs-federation-postgres-partial.md |
| UF-17 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-17-tests-policy-enforcement-ignored | 0285ee1812bfeffd93eeed2ca3bdc9895c55ccaf | reports/unfinished_feature_audit/staging/UF-17-tests-policy-enforcement-ignored.md |
| UF-18 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-18-tests-gpu-backend-coverage-ignored | f54edc079650df0d0876fb7ea1fc799dbf579b2e | reports/unfinished_feature_audit/staging/UF-18-tests-gpu-backend-coverage-ignored.md |
| UF-19 | 5f5f60d642db551395037fe13aba0bb2405dcd4d -> c/stage/uf-19-tests-mlx-memory-management-stubbed | 08c23e8afca2e90e7ece7b67211bcc4c5f60cc9e | reports/unfinished_feature_audit/staging/UF-19-tests-mlx-memory-management-stubbed.md |

## Rationales
- UF-01 (unfinished): unimplemented! blocks full boot invariant config harness assembly in integration tests.
- UF-02 (partial): Public example snippet is non-runnable and requires manual fill-in.
- UF-03 (unfinished): Quickstart code path leaves required AppState constructor unresolved.
- UF-04 (unfinished): Deletion logic logs and mutates cache but does not integrate with a real Linux keychain backend.
- UF-05 (speculative): Provider explicitly simulates behavior locally and is not wired to production GCP KMS auth flow.
- UF-06 (partial): Attestation path can emit synthetic signature bytes instead of a cryptographic signature.
- UF-07 (speculative): Bypass mode returns placeholder signatures that are intentionally non-production.
- UF-08 (partial): Current K value is not sourced from lifecycle state, risking stale pressure actions.
- UF-09 (unfinished): macOS callback registration is stubbed and does not wire real callback hooks.
- UF-10 (partial): Disk read/write counters are hardcoded to zero as temporary placeholders.
- UF-11 (unfinished): Security pack can return empty/no-op results when offline stub path is used.
- UF-12 (partial): Config constant remains static while TODO indicates runtime-sourced behavior is pending.
- UF-13 (partial): Saved settings exist but are not wired into the intended UI consumers.
- UF-14 (unfinished): Test script logs unsupported non-SQLite probe path as not implemented.
- UF-15 (speculative): Debug verification path checks placeholder behavior rather than production worker execution.
- UF-16 (partial): Spec explicitly states current federation/postgres multi-node gaps.
- UF-17 (unfinished): Core policy scenarios are tracked but intentionally disabled in CI/test runs.
- UF-18 (unfinished): Critical backend selection/discovery checks are ignored pending testability refactor.
- UF-19 (unfinished): Integration test module remains placeholder while API updates are pending.

## Artifacts
- reports/unfinished_feature_audit/2026-02-12-feature-audit.md
- reports/unfinished_feature_audit/2026-02-12-move-ledger.tsv
- reports/unfinished_feature_audit/staging/UF-*-*.md (one file per staging branch commit)
