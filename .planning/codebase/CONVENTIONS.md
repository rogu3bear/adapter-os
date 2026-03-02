# adapterOS Code Conventions

## Quality gates
- Run `cargo fmt --all` and `cargo clippy --workspace -- -D warnings` before pushing changes; those commands are canonically listed under the **Quality** section of `CONTRIBUTING.md` so reviewers can assume every PR follows them before merge.
- Keep dependencies tight: `deny.toml` denies vulnerabilities, unknown registries, unlicensed packages, and the rest of the cargo-deny buckets so new packages must comply before landing.
- Validate determinism-sensitive changes with targeted suites from `DETERMINISM.md`: `cargo test --test determinism_core_suite`, `cargo test -p adapteros-lora-router --test determinism`, and `cargo test -p adapteros-server-api --test replay_determinism_tests`.

## Naming & structure
- Align names with the Naming policy pack described in `docs/POLICIES.md` (PolicyId 23) so features, adapters, and tests keep the established nouns, kebab-case commands, and meaning preserved across the repo.
- When adding benchmarks, follow the `{category}_benchmarks.rs` file-naming pattern documented in `tests/benchmark/README.md`, and keep the folder/package layout consistent with the existing Criterion-based suites.
- Prefer crate-scoped ownership under `crates/` and keep cross-cutting docs under `docs/` and `.planning/`; do not create parallel top-level layout patterns when an existing crate or doc area already matches.

## Path hygiene & scope
- Write runtime artifacts only under `./var/` (and never create another `var/` or `tmp/` inside a crate) and avoid `/tmp`, `/private/tmp`, and `/var/tmp`; this is reiterated in the **Path Hygiene** section of `CONTRIBUTING.md` to keep builds portable.
- Aim for minimal diffs and reuse existing patterns; the **Scope** section of `CONTRIBUTING.md` explicitly discourages parallel abstractions and urges a clear justification before invoking expensive actions such as workspace-wide tests or `cargo clean`.
- Document new patterns in their natural files rather than inventing parallel paths—this repo prefers extending existing crates and directories so that later agents can find behavior by searching `rg` before adding new surfaces.

## Error handling & observability
- Follow the existing `Result<T>`-first style in policy and server crates (for example `crates/adapteros-policy/src/policy_packs.rs` and `crates/adapteros-policy/src/cve_client.rs`) and avoid panic-driven control flow outside explicit test code.
- Use structured `tracing` logs with contextual fields (`info!`, `warn!`, `error!`) as seen in `crates/adapteros-policy/src/cve_client.rs` and `crates/adapteros-policy/src/unified_enforcement.rs`; include identifiers (tenant/request/backend) when available.
- Keep API and policy payload types serializable with `serde` derives following patterns in `crates/adapteros-policy/src/security_response.rs` and `crates/adapteros-policy/src/policy_packs.rs`.

## Testing conventions
- Prefer targeted test invocations for changed areas before broad suites, aligning with `CONTRIBUTING.md` and existing focused tests in `tests/` and crate-level `tests/` modules.
- Use `#[tokio::test]` for async paths and `#[test]` for sync/unit coverage, matching files like `tests/adapter_stress_tests.rs` and `tests/kv_residency_quota_integration.rs`.
- Keep benchmark additions aligned with `tests/benchmark/README.md` and register new benchmarks in `tests/benchmark/Cargo.toml` using the existing `[[bench]]` convention.
