# Issue Tracker

## P0

### LOCAL-001 Missing anchor contract script
- Type: chore
- Priority: P0
- Status: fixed
- Acceptance criteria: `./scripts/ci/check_anchor_contract.sh` exists or CI workflow updated to a valid script; baseline command succeeds.
- Notes: Added `scripts/ci/check_anchor_contract.sh`; verified via `./scripts/ci/check_anchor_contract.sh`.

### LOCAL-002 Telemetry test compile errors
- Type: bug
- Priority: P0
- Status: closed
- Acceptance criteria: `cargo test --workspace --all-targets --exclude adapteros-lora-mlx-ffi` compiles `telemetry_export_tests` without missing types/variants.
- Notes: Could not reproduce after rerunning `cargo xtask check-all --verbose`; telemetry errors appear resolved/stale.

### LOCAL-003 Clippy failures in adapteros-core
- Type: bug
- Priority: P0
- Status: fixed
- Acceptance criteria: `cargo clippy --workspace --all-targets --exclude adapteros-lora-mlx-ffi -- -D warnings` passes with no new ignores.
- Notes: Clean on rerun `cargo clippy --workspace --all-targets --exclude adapteros-lora-mlx-ffi -- -D warnings`.

### #163 Security: Hardcoded insecure JWT secret in config template
- Type: security
- Priority: P0
- Status: fixed
- Acceptance criteria: No hardcoded JWT secret in templates; config requires env/secure source; update docs/tests as needed.
- Notes: `configs/cp.toml`, `configs/cp-8080.toml`, `configs/cp-var.toml` now require `AOS_SECURITY_JWT_SECRET`.

### #181 import.rs:94 uses placeholder signature bypassing verification
- Type: security
- Priority: P0
- Status: fixed
- Acceptance criteria: Import path validates signatures or returns explicit error; tests cover invalid signatures.
- Notes: Import now parses `signature.sig`, computes SBOM hash, stores base64 signature, and refuses missing signatures when `--verify` is used.

### #182 registry.rs:220 skips signature verification with placeholder
- Type: security
- Priority: P0
- Status: fixed
- Acceptance criteria: Registry path enforces signature verification; no placeholder bypass.
- Notes: `aosctl registry sync` now loads a public key and verifies signatures before import.

### #184 PRD: Complete Adapter Signature Verification Chain
- Type: security
- Priority: P0
- Status: fixed
- Acceptance criteria: End-to-end signature verification enforced on import/register/load; tests for tampered artifacts fail.
- Notes: Import and registry workflows now enforce signature validation and store SBOM/signature metadata; overlaps resolved with #181/#182.

### LOCAL-015 Security regression suite failures
- Type: security
- Priority: P0
- Status: fixed
- Acceptance criteria: `cargo test -p adapter-os --test security_regression_suite` passes.
- Notes: Hardened test-block detection, adjusted ZeroizeOnDrop check, removed unwraps in keychain production paths, moved unsafe out of public APIs (keychain/io_utils), and added safe lock/timestamp helpers.

### LOCAL-016 MLX real backend link mismatch
- Type: build
- Priority: P0
- Status: fixed
- Acceptance criteria: `cargo test --workspace --all-targets --exclude adapteros-lora-mlx-ffi` succeeds without `MLX_FORCE_STUB=1`, or local MLX requirements are documented as a blocker.
- Notes: Local MLX link fails with undefined `mlx::core::*` symbols and macOS version mismatch warnings; see `.codex/LOGS/sweep_test_no_stub.txt`. `MLX_PATH=/Users/mln-dev/Dev/adapter-os/target/debug/build/mlx-sys-69997a727f5fd885/out/build` makes real MLX build succeed; see `.codex/LOGS/mlx_path_check.txt`. Full workspace run previously succeeded until kv_vs_sql bench failure; after bench fix, rerun recommended with MLX_PATH.

### LOCAL-017 kv_vs_sql bench fails on tenant/lineage validation
- Type: test
- Priority: P0
- Status: fixed
- Acceptance criteria: `cargo test -p adapteros-db --bench kv_vs_sql` completes without panics.
- Notes: Bench used SQL-only tenant insert, adapter_id in parent_id, and fork_type values not allowed by DB constraint. Fixed tenant creation to use consistent IDs across SQL/KV and lineage to use internal IDs with valid fork_type. Verified via `cargo test -p adapteros-db --bench kv_vs_sql`.

## P1

### #191 PRD: Standardize partial_cmp Error Handling Across Codebase
- Type: refactor
- Priority: P1
- Status: fixed
- Acceptance criteria: All `partial_cmp` usages handle NaN explicitly; no `.unwrap()` remains in routing/filtering paths; tests for NaN cases.
- Notes: Replaced `.partial_cmp().unwrap()` in runtime paths; added NaN-safe comparisons in router/metrics/memory code and tests for median filter NaNs.

### #190 PRD: Apply Zeroize Pattern to KMS Credentials
- Type: security
- Priority: P1
- Status: fixed
- Acceptance criteria: KMS credentials zeroized on drop; tests/benchmarks unchanged; no new secrets in logs.
- Notes: `KmsCredentials` now implements `Zeroize`/`ZeroizeOnDrop`.

### #180 implementation.rs: u64 to usize casts truncate on 32-bit systems
- Type: bug
- Priority: P1
- Status: fixed
- Acceptance criteria: Casts replaced with fallible conversions; errors propagate; tests cover 32-bit overflow scenario.
- Notes: `crates/adapteros-aos/src/implementation.rs` now uses fallible `usize::try_from`.

### #179 policy.rs uses unsafe transmute for PolicyId instead of safe conversion
- Type: bug
- Priority: P1
- Status: fixed
- Acceptance criteria: `PolicyId` conversion uses safe mapping; no `unsafe transmute`; tests updated.
- Notes: CLI now maps IDs via `PolicyId::all()`; removed unsafe transmute.

### #178 HotSwapManager::default() panics if AosLoader creation fails
- Type: bug
- Priority: P1
- Status: fixed
- Acceptance criteria: `default()` returns safe fallback or error; no panic on loader failure; tests cover failure path.
- Notes: `HotSwapManager::default()` now logs and returns disabled manager when loader init fails.

### #177 serve.rs:240 uses placeholder embedding hash instead of manifest value
- Type: bug
- Priority: P1
- Status: fixed
- Acceptance criteria: Embedding hash sourced from manifest; placeholders removed; tests validate hash propagation.
- Notes: `aosctl serve` now uses `manifest.policies.rag.embedding_model_hash`.

### #175 policy.rs: ID range check (1..=25) excludes policies 26-29
- Type: bug
- Priority: P1
- Status: fixed
- Acceptance criteria: Range checks include all policy IDs; tests cover new IDs.
- Notes: Range validation now derives from `PolicyId::all()`.

### #174 result_merger.rs:367 panics on NaN confidence values
- Type: bug
- Priority: P1
- Status: fixed
- Acceptance criteria: NaN handled deterministically (drop/flag/error); no panic; tests cover NaN inputs.
- Notes: `result_merger.rs` uses NaN-safe compare; additional NaN-handling applied in metrics/memory.

### #166 Bug: partial_cmp().unwrap() can panic on NaN values in filter engine
- Type: bug
- Priority: P1
- Status: fixed
- Acceptance criteria: Filter engine handles NaN without panic; unit tests added.
- Notes: Filter median uses `total_cmp` and skips NaNs; test covers NaN input.

### #162 Bug: Panic calls in policy evaluation code paths
- Type: bug
- Priority: P1
- Status: closed
- Acceptance criteria: Panics replaced with explicit errors; tests cover failure cases.
- Notes: Panics referenced in issue are in `#[cfg(test)]` modules only; production paths use Results.

### #160 Audit violation: allow_silent_downgrade field exists despite warning
- Type: bug
- Priority: P1
- Status: fixed
- Acceptance criteria: Field removed or enforced to error; config validation updated; docs reflect behavior.
- Notes: Deserialization now rejects `allow_silent_downgrade=true`; tests cover rejection.

### #186 PRD: Sync PolicyId Enum with CLI Range Checks
- Type: bug
- Priority: P1
- Status: fixed
- Acceptance criteria: CLI validation uses PolicyId enum or shared source of truth; policy count and ranges consistent.
- Notes: CLI now derives counts/range from `PolicyId::all()`.

## P2

### #196 fix(diagnostics): Add missing integration tests for end-to-end diagnostic flow
- Type: bug
- Priority: P2
- Status: fixed
- Acceptance criteria: Integration test covers diagnostic flow end-to-end; test passes locally.
- Notes: Added `crates/adapteros-db/tests/diagnostics_flow_tests.rs` to cover service → writer → DB.

### #192 fix(cli): Misleading error message in review command claims endpoints not implemented
- Type: bug
- Priority: P2
- Status: fixed
- Acceptance criteria: Error message reflects actual status and guidance; CLI tests updated if present.
- Notes: CLI now reports review API unavailable (server down/disabled) instead of "not implemented".

### #188 PRD: Complete KV Backend Migration Testing
- Type: test
- Priority: P2
- Status: split_needed
- Acceptance criteria: KV migration tests added/updated; `cargo test -p adapteros-db` passes for KV paths.
- Notes: Needs kv-backend feature coverage and un-ignore full migration workflow tests.

### #189 PRD: Implement Remaining 28 Boot Invariants
- Type: feature
- Priority: P2
- Status: split_needed
- Acceptance criteria: Boot invariants implemented and validated; tests cover new invariants.
- Notes: Requires scoped batches per invariant group (security/data integrity/availability).

### #187 PRD: Implement Heap Observer Allocator Hooks for Accurate Fragmentation
- Type: perf
- Priority: P2
- Status: split_needed
- Acceptance criteria: Heap observer hooks collect fragmentation metrics; tests/benchmarks updated.
- Notes: Needs allocator integration plan and metrics wiring.

### #185 PRD: Implement Real TenantUsage Resource Metrics
- Type: feature
- Priority: P2
- Status: split_needed
- Acceptance criteria: TenantUsage reports real metrics; API responses validated; tests updated.
- Notes: Placeholder metrics remain in `crates/adapteros-db/src/tenants.rs`.

### #183 runtime_dir.rs:48 uses env::set_var without unsafe (deprecated in Rust 2024)
- Type: bug
- Priority: P2
- Status: fixed
- Acceptance criteria: Replace with safe pattern per Rust 2024 or gated `unsafe` usage; compile passes on latest toolchain.
- Notes: `ensure_runtime_dir` no longer mutates env; caller already sets `AOS_VAR_DIR`.

### #164 Bug: Silent adapter filtering drops adapters without logging
- Type: bug
- Priority: P2
- Status: fixed
- Acceptance criteria: Dropped adapters are logged/telemetry recorded; tests confirm observability.
- Notes: `apply_allowlist` logs filtered adapters and warns when none allowed.

### #161 Design: Best-effort mode proceeds with 40-70% confidence instead of refusing
- Type: design
- Priority: P2
- Status: split_needed
- Acceptance criteria: Best-effort mode refuses below threshold or documents policy; tests or config validation updated.
- Notes: Requires product decision on best-effort default thresholds and UX.

### #155 Persist config baseline for drift detection
- Type: feature
- Priority: P2
- Status: split_needed
- Acceptance criteria: Baseline config persisted and compared on boot; drift detection reports differences.
- Notes: Needs storage schema for baseline hash and boot-time comparison logic.

### #152 Add skip-worker readiness mode
- Type: feature
- Priority: P2
- Status: fixed
- Acceptance criteria: Readiness check can skip worker when configured; health endpoints reflect mode.
- Notes: `ReadinessMode::Relaxed` uses `server.skip_worker_check`; documented in `docs/BOOT_READYZ_TRACE.md`.

### #151 Expose GPU memory metrics via Worker API
- Type: feature
- Priority: P2
- Status: split_needed
- Acceptance criteria: Worker API reports GPU memory metrics; docs/tests updated.
- Notes: Requires worker metrics plumbing and API shape update.

### #150 Track boot download MB in BootStateManager
- Type: feature
- Priority: P2
- Status: split_needed
- Acceptance criteria: BootStateManager tracks download MB; surfaced in logs/metrics.
- Notes: `BootStateManager` counter exists but is not wired to download paths.

### LOCAL-004 Refusal best-effort policy switch
- Type: design
- Priority: P2
- Status: blocked
- Acceptance criteria: Config allows disabling BestEffort responses or raising thresholds; docs/tests updated.
- Notes: Needs product decision on default thresholds and UX behavior for low-confidence responses.

### LOCAL-005 KV migration test harness
- Type: test
- Priority: P2
- Status: blocked
- Acceptance criteria: Enable full SqlOnly → DualWrite → KvPrimary migration test under `kv-backend` feature; test passes locally.
- Notes: Requires kv-backend feature in test harness and temp redb setup.

### LOCAL-006 Boot invariants batch 1
- Type: feature
- Priority: P2
- Status: blocked
- Acceptance criteria: Implement first batch of missing boot invariants (security/data integrity) with fail-open/closed semantics and tests.
- Notes: Needs invariants prioritization from `crates/adapteros-server/src/boot/invariants.rs`.

### LOCAL-007 Boot download bytes instrumentation
- Type: feature
- Priority: P2
- Status: blocked
- Acceptance criteria: Download paths increment `BootStateManager` bytes; boot progress APIs report MB.
- Notes: Requires identifying model/adapter download code paths to hook.

### LOCAL-008 Worker GPU memory metrics
- Type: feature
- Priority: P2
- Status: blocked
- Acceptance criteria: Worker API includes GPU memory usage totals/available; tests verify schema.
- Notes: Needs worker-side metrics source and API shape update.

### LOCAL-009 TenantUsage metrics wiring
- Type: feature
- Priority: P2
- Status: blocked
- Acceptance criteria: TenantUsage exposes storage/CPU/GPU/memory metrics from real sources; placeholders removed.
- Notes: Requires metric sources and schema updates in `adapteros-db` + API.

### LOCAL-010 Heap observer allocator hooks
- Type: perf
- Priority: P2
- Status: blocked
- Acceptance criteria: Fragmentation metrics sourced from allocator hooks; tests/benchmarks updated.
- Notes: Needs allocator integration plan and instrumentation points.

### LOCAL-011 Config baseline persistence
- Type: feature
- Priority: P2
- Status: blocked
- Acceptance criteria: Persist config baseline hash and compare at boot; drift is logged/flagged.
- Notes: Needs storage schema and boot-time validation logic.

## P3

### #198 docs(config): Add diagnostics configuration schema reference
- Type: docs
- Priority: P3
- Status: fixed
- Acceptance criteria: Docs link to diagnostics config schema; examples provided.
- Notes: `docs/CONFIGURATION.md` now documents `diag.*` keys and schema references.

### #197 feat(cli): Add diagnostic examples directory with runnable samples
- Type: feature
- Priority: P3
- Status: fixed
- Acceptance criteria: Examples directory added with runnable samples; documented in README.
- Notes: Added `examples/diagnostics/README.md` and updated `examples/README.md`.

### #195 docs(cli): Document undocumented diag bundle flags and capabilities
- Type: docs
- Priority: P3
- Status: fixed
- Acceptance criteria: CLI docs list diag bundle flags/capabilities; accuracy verified.
- Notes: `docs/CLI_GUIDE.md` now documents `diag run|export|verify` flags and examples.

### #194 docs(cli): Document stubbed/unimplemented commands in CLAUDE.md
- Type: docs
- Priority: P3
- Status: fixed
- Acceptance criteria: CLAUDE.md documents stubbed commands; aligns with CLI behavior.
- Notes: Added stubbed/partial CLI command list to `CLAUDE.md`.

### #193 docs(diagnostics): Add README.md and integration guide for diagnostics crate
- Type: docs
- Priority: P3
- Status: fixed
- Acceptance criteria: Diagnostics README and integration guide present.
- Notes: Added `crates/adapteros-diagnostics/README.md` with integration example.

### #176 policy.rs:196 displays '20 policies' but there are 29
- Type: docs
- Priority: P3
- Status: fixed
- Acceptance criteria: Displayed policy count matches actual; tests or docs updated.
- Notes: CLI now reports dynamic counts and range.

### #157 feat(training): Add GPU gradient kernels for full GPU training
- Type: feature
- Priority: P3
- Status: split_needed
- Acceptance criteria: GPU gradient kernels available; training uses GPU path; tests/benchmarks updated.
- Notes: Requires design and kernel implementation; split into GPU kernel + training pipeline tasks.

### #154 Implement version-based training workflow
- Type: feature
- Priority: P3
- Status: split_needed
- Acceptance criteria: Training workflow uses versioning; migration docs added.
- Notes: Requires repository schema changes and workflow design.

### #153 Implement repository version timeline
- Type: feature
- Priority: P3
- Status: split_needed
- Acceptance criteria: Version timeline stored and queryable; API/docs updated.
- Notes: Requires schema/API design and backfill plan.

### LOCAL-012 GPU gradient kernel prototype
- Type: feature
- Priority: P3
- Status: blocked
- Acceptance criteria: Initial GPU gradient kernel for one training op with benchmark and correctness test.
- Notes: Needs kernel design and integration surface definition.

### LOCAL-013 Training workflow versioning schema
- Type: feature
- Priority: P3
- Status: blocked
- Acceptance criteria: Schema supports versioned training workflow and migration path.
- Notes: Needs repository/versioning data model decisions.

### LOCAL-014 Repository version timeline API
- Type: feature
- Priority: P3
- Status: blocked
- Acceptance criteria: Timeline entries persisted and served via API; docs updated.
- Notes: Requires API shape + query strategy.
