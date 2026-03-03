# Codebase Structure for adapterOS Planning

This map records where each major concern lives so new work follows existing conventions and avoids duplicate footprints.

## Workspace roots
- The workspace is defined in `Cargo.toml` (root) with `members = ["crates/*"]`, so every Rust crate lives under `crates/`. Add new crates there and keep them listed in `Cargo.toml` alongside the hundreds of existing modules (`adapteros-server`, `adapteros-lora-worker`, `adapteros-ui`, etc.).
- Surface-level governance files like `README.md`, `QUICKSTART.md`, `VERSION`, `CODE_OF_CONDUCT.md`, and `POLICIES.md` sit at the root; new policy or onboarding docs belong adjacent to them, and the planning artifacts under `.planning/` hold forward-looking intel (e.g., this file).

## Control plane & API directories
- `crates/adapteros-server/` houses the boot runner plus `src/boot/`. Extend `boot` submodules and add config helpers here when you need new phases, invariants, or migrations (`boot/initialize_*` and `run_migrations`).
- `crates/adapteros-server-api/` contains the Axum router (`routes/`), middleware chain builder (`middleware/chain_builder.rs`), inference core (`inference_core/`), telemetry helpers (`telemetry/`), and UDS client (`uds_client.rs`). New handlers, middleware, or policy hooks should be placed inside these folders rather than scattered elsewhere.
- `crates/adapteros-policy/` and `POLICIES.md` hold the `PolicyId` list and enforcement metadata; policy authors should update both the Rust registry and the markdown guidance together.

## Data plane & inference backends
- Worker binaries live under `crates/adapteros-lora-worker/` (entry in `src/lib.rs`, `src/uds_server.rs`, test suites, `training/`). Hook new worker RPCs or lifecycle hooks there and only add backend orchestration logic inside the worker crate or the per-backend crates listed below.
- Adapter selection is centralized in `crates/adapteros-lora-router/` (`quantization.rs` contains the K-sparse scoring logic). Keep changes to scoring/seed derivation localized to this crate and call them from `adapteros-server-api::inference_core`.
- GPU/backends are split across `crates/adapteros-lora-mlx-ffi`, `crates/adapteros-lora-kernel-mtl`, and `crates/adapteros-lora-kernel-coreml`. Add new kernels to the corresponding crate and plug them into the worker dispatcher in `crates/adapteros-lora-worker/src/lib.rs`.
- Lifecycle helpers like hot-swap, eviction, and adapter registry interaction live under `crates/adapteros-lora-lifecycle`, `crates/adapteros-registry`, and `adapters/` (the on-disk SQLite registry). If you introduce a new adapter lifecycle state machine, put tests in `crates/adapteros-lora-lifecycle/tests` and extend the registry migrations in `migrations/`.

## Supporting infrastructure
- Configuration files live in `configs/` (examples: `configs/cp.toml`, `configs/dev.toml`, `configs/production-multinode.toml`). Pair each new config profile with a doc mention and reference to `crates/adapteros-config/src/lib.rs` so the parser can load it.
- Runtime data is kept under `var/` (`var/run/aos/...` sockets, `var/adapters/`, `var/aos-cp.sqlite3`, `var/telemetry/`). Do not commit these files; mutate them through the boot/worker logic that writes into `var/` according to `crates/adapteros-config/src/path_resolver.rs`.
- Generated operator reports and ad-hoc audits should write under `var/reports/` (not root `reports/`) to keep generated artifacts in one runtime tree.
- Migrations go into `migrations/` (SQL + Rust), and `crates/adapteros-db` provides the helpers used by `adapteros-server`'s `run_migrations`. When schema changes are needed, add a migration file plus an entry in `crate::migrations` to keep the SQLite checkout consistent.

## UI, CLI, and automation
- UI assets live inside `crates/adapteros-ui/` with dependencies recorded in `package-lock.json`/`node_modules/`. The CLI front-end runs in `crates/adapteros-cli/` and the standalone `aosctl/aosctl` script. Add new CLI surfaces in `crates/adapteros-cli/src/commands/`.
- Tooling and automation scripts sit in `scripts/`, `build_support/`, `tools/`, `deploy/`, and `manifests/`. Keep environment bootstraps inside `scripts/` (use `scripts/contracts/check_all.sh` for contract checks and the Codex GSD health script for `.planning` integrity) and CI or deployment glue inside `deploy/`/`manifests/`.
- Monitoring, telemetry exporters, and guardrails span `monitoring/`, `baseline_fingerprint.*` under `var/`, and the crate `crates/adapteros-metrics-exporter`. When adding observability output, update both the exporter crate and any dashboards in `monitoring/`.

## Tests, examples, and docs
- Integration and regression tests live in `tests/`, `test/`, `test_data/`, `golden_runs/`, `mocks/`, `fuzz/`, and the nested `crates/*/tests`/`benches`. Choose the directory that matches the scope (e.g., `test_data/` for fixtures, `golden_runs/` for expected outputs).
- Examples and reusable fixtures live in `examples/` and `mocks/`. Golden deterministic fixtures live under `golden_runs/baselines/`.
- Documentation clusters in `docs/` (`docs/ARCHITECTURE.md`, `docs/MLX_GUIDE.md`), `docs/POLICIES?` etc., plus dedicated planning files under `.planning/`. Add new guides close to the concept and keep them referenced from `ROADMAP.md` or `PROJECT.md` when appropriate.

## Legacy and transitional roots
- Legacy root documentation for retired `baselines/`, `codegen/`, `commands/`, `skills/`, and `etc/` now lives under `docs/legacy/`.
- `metal/baselines/kernel_hash.txt` is the canonical kernel-hash baseline path for Metal determinism checks.
- `target/codegen/` is the active generated OpenAPI/codegen scratch path used by contract/CI checks.
- `golden_runs/` is the canonical baseline location; avoid introducing `var/golden_runs` references in production paths.

## TODO + Agent teams
- [ ] Runtime Core → `crates/adapteros-server`, `crates/adapteros-server-api`, `DETERMINISM.md`: confirm each boot change updates this guide and the deterministic operating instructions.
- [ ] Data Plane → `crates/adapteros-lora-worker`, `crates/adapteros-lora-router`, `var/run/aos`: sketch an abridged flow diagram in `docs/ARCHITECTURE.md` whenever a new backend is added.
- [ ] QA + Integration → `tests/`, `var/playwright/runs/*/test-results/`, `monitoring/`: add a concrete verification step for every binary-level change so we keep the `var`+`db` state consistent.

Structure governance lives next to the files mentioned above; follow the referenced paths before branching out.
