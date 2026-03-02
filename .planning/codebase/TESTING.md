# adapterOS Testing Practices

## Overview
- The centralized test guide in `tests/README.md` (last updated 2026-01-27) lists targeted coverage for unit, integration, concurrency, schema, and policy validation and advertises 75%+ coverage on critical paths.
- Follow the workspace-level `cargo test --workspace` and the package-specific runs under the **Test** section of `CONTRIBUTING.md` (`cargo test -p adapteros-lora-router`, `cargo test -p adapteros-core`, `cargo test -p adapteros-policy`).

## Core suites (see `tests/README.md`)
### 1. Unit tests
- Live next to the code (`crates/*/src/**/*.rs`) and validate small helpers such as router k-sparse selection or BLAKE3 hashing.
- Run `cargo test -p adapteros-lora-router` (and the other packages listed in `CONTRIBUTING.md`) for targeted proofs before a wider integration run.

### 2. Integration tests
- Located in `tests/*.rs`; `adapter_hotswap.rs`, `concurrency.rs`, `determinism_tests.rs`, `gpu_verification_integration.rs`, `load_hotswap.rs`, `worker_mocked_components.rs`, and `stability_reinforcement_tests.rs` each map to concrete features in the table inside `tests/README.md`.
- Use `cargo test --test <test_file_name>` and add `--features extended-tests` when the table flags it (e.g., `adapter_hotswap`, `worker_mocked_components`, `server_lifecycle_tests`).

### 3. Schema validation
- `crates/adapteros-db/tests/schema_consistency_tests.rs` keeps migrations aligned with SQL and struct definitions; run it with `cargo test -p adapteros-db schema_consistency_tests`.

### 4. Hot-swap
- `tests/adapter_hotswap.rs` covers preload/swap cycles, stress loops, determinism, and memory leak assertions; execute it via `cargo test --test adapter_hotswap --features extended-tests` per the guide.

### 5. Concurrency
- `tests/concurrency.rs` exercises race conditions and data races, and the guide specifies Loom model checking plus Miri or ThreadSanitizer where available; run `cargo test --test concurrency` and deploy Loom scripts to iron out interleavings.

### 6. Server lifecycle
- Ensure `cargo build -p adapteros-server` runs before `tests/server_lifecycle_tests.rs` and that the suite is invoked as `cargo test --test server_lifecycle_tests --features extended-tests`.
- Key expectations include `test_server_startup_success`, `test_server_startup_missing_database`, `test_server_port_conflict`, `test_graceful_shutdown_sigterm`, `test_config_reload_sighup`, and `test_health_check_degradation` (all listed in `tests/README.md`).

## Feature flags & tooling
- `extended-tests` unlocks heavy scenarios documented in `tests/README.md`; tie feature selections to the table so long-running suites run only when explicitly requested.
- Loom, Miri, and future ThreadSanitizer runs are highlighted under **Concurrency Testing**, so gate them to targeted experiments rather than default workflow.

## Benchmarking
- Follow the `tests/benchmark/README.md` playbook when adding Criterion suites: place files in `benches/`, name them `{category}_benchmarks.rs`, register them with `[[bench]]` in `Cargo.toml`, and keep warm-up/sample/isolation/documentation discipline.
- The benchmark guide also covers common issues (Metal availability, memory limits) and performance tips (dedicated hardware, SSDs, cooling), which are worth checking before scaling a run.

## Troubleshooting & known issues
- Refer to the troubleshooting sections of `tests/README.md` for general addons and the “Known Issues” note (date-stamped 2025-11-19) about server compilation blockers before relying on lifecycle suites.
- When debugging inconsistent results, follow the concurrency checklist (idle system, Loom runs, deterministic inputs) plus the benchmark tips for stabilizing measurements.

## Verification pointers
- Combine the workspace checks from `CONTRIBUTING.md` with the targeted commands in `tests/README.md` so each change trains unit, integration, schema, hot-swap, and concurrency protections before release.
- Document any skipped suites (e.g., Loom or Metal benchmarks) along with the reason and what remains unverified to keep reviewers aware.
