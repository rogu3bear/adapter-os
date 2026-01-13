# Repository Guidelines

## Project Structure & Module Organization
- Rust workspace under `crates/`: `adapteros-core` (seed, determinism), `adapteros-server` (control plane), `adapteros-lora-*` (router, worker, kernels), `adapteros-ui` (Leptos WASM), `adapteros-tui` (terminal dashboard). Tools and helper binaries in `xtask/`, `aosctl`, and `tools/`.
- Frontend lives in `crates/adapteros-ui` (Trunk build, Rust-first types via `adapteros-api-types`); built WASM is served from `static/` by the backend. Terminal UI is in `crates/adapteros-tui` (ratatui + crossterm).
- Config/infrastructure: `configs/`, `deploy/`, `scripts/` (CI, build, env setup), `metal/` (kernels), `migrations/` (DB), shared fixtures in `test_data/`, integration tests in `tests/`, documentation in `docs/`. Runtime files land in `var/` (logs, pids, sockets).
- Reference docs to skim: `README.md`, `QUICKSTART.md`, `CONTRIBUTING.md`, `DETERMINISM_LOOP_IMPLEMENTATION_SUMMARY.md`, `docs/API_TYPE_GENERATION.md`, and `scripts/ci/check_openapi_drift.sh` usage.

## Entry Points: ./start, CLI, and TUI
- `./start up` (default): primary orchestrator; runs bannered preflight, backend via `scripts/service-manager.sh`, optional worker, readiness probe (`/readyz`), and offers `aosctl` install if missing. Reads `.env` (non-overriding) and writes logs to `var/logs/start.log`. `./start down` stops services cleanly; `./start status` queries service-manager.
- Flags: `--skip-worker`, `--quick` (skip readyz wait), `--worker-model <path>`, `--worker-tokenizer <path>`, `--worker-backend <mlx|coreml|metal>`, `--verbose`, `--trace`. Set `AOS_DEV_NO_AUTH=1 ./start up` for UI exploration without touching RBAC code.
- CLI: build with `cargo build --release -p adapteros-cli --features tui` and `ln -sf target/release/aosctl ./aosctl`; commands like `aosctl models seed --model-path <dir>` and `aosctl tui` for the terminal dashboard when built with `--features tui`.
- TUI: `cargo run -p adapteros-tui` (ratatui) for live services/logs/config. Quick keys: `b` boot, `s` services, `l` logs, `m` metrics, `c` config, `q` quit. Uses REST on server port and metrics endpoints; falls back to mock data if offline.

## Local Dev, Build, and Run
- Backend dev: `./start up` (auth on) or `AOS_DEV_NO_AUTH=1 ./start up`. UI dev: `cd crates/adapteros-ui && trunk serve` and `cd crates/adapteros-ui && trunk build --release` for release WASM served from `static/`.
- Cargo-first workflow: prefer direct `cargo`/`./start` commands.
- Primary environment: macOS on Apple Silicon (M4 Max); we intend to go fully Mac‑first, so favor macOS tooling/assumptions unless explicitly asked to keep cross‑platform.
- Fresh builds: `./scripts/fresh-build.sh`, then `cargo build --release --locked --offline`, then `./scripts/build_metadata.sh`, `./scripts/record_env.sh`, `./scripts/strip_timestamps.sh`. Metal shaders: `cd metal && bash build.sh`. CLI-only: `cargo build --release -p adapteros-cli --features tui`.
- Targeted iteration: `cargo check -p <crate>`, `cargo sqlx prepare` for DB query changes. Set `AOS_VAR_DIR` to redirect runtime outputs, `AOS_LOG_PROFILE=trace` for verbose boot logging.

## Testing & Quality Gates
- Suites: `bash scripts/test/all.sh all` (fmt, clippy, Rust + Leptos), `bash scripts/test/all.sh rust`, `bash scripts/test/all.sh ui`, ignored tests via `cargo test --workspace --features extended-tests -- --ignored`, hardware tests via the explicit commands in `docs/stability/CHECKLIST.md`. Debug with `cargo test -- --nocapture` or `--test-threads=1`.
- Determinism: run `cargo test --test determinism_core_suite -- --test-threads=8`, `cargo test -p adapteros-lora-router --test determinism`, and `bash scripts/check_fast_math_flags.sh` whenever touching routing, seeds, kernels, or math. Use `AOS_DEBUG_DETERMINISM=1` to trace seed inputs and router tie-breaks.
- OpenAPI drift: `./scripts/ci/check_openapi_drift.sh --fix` and commit `docs/api/openapi.json` after API surface changes. Keep generated clients aligned (TypeScript client at `ui/src/api/generated.ts` when present; Leptos UI uses shared Rust types).
- Formatting/lint: `cargo fmt --all`, `cargo clippy --workspace -D warnings`. Run determinism + SBOM checks explicitly when needed.

## Coding Style, Determinism, and Naming
- Rust naming (`snake_case` items, `CamelCase` types); prefer `Result` for fallible flows; use `tracing` over `println!`. Keep Leptos components small; reuse `components/` and `hooks/`.
- Determinism invariants: HKDF-SHA256 with BLAKE3 global seed (`crates/adapteros-core/src/seed.rs`); router sorts score DESC with index ASC tie-break, Q15 denominator `32767.0` (`crates/adapteros-lora-router/src/constants.rs`); forbid `-ffast-math` flags. Maintain reproducible Metal builds (strip timestamps via `./scripts/strip_timestamps.sh` in release builds).

## UI Workflow (Current Focus)
- Goal: get the Leptos UI visible and iterate on visuals; avoid backend/CI changes unless required. Use `AOS_DEV_NO_AUTH=1 ./start up` to explore pages without altering RBAC logic.
- Shared API types come from Rust crates; avoid ad-hoc JSON types. For new UI state, prefer signals/hooks; keep components deterministic and side-effect free.
- Add screenshots/gifs for UI PRs; ensure `cd crates/adapteros-ui && trunk build --release` passes for release artifacts.

## Security & Configuration Tips
- Secrets: never commit; use platform keychain providers. Follow `WORKER_SETUP.md` and `SECURITY.md` for hardened paths. `.env` is optional and non-overriding; export vars before `./start` to take precedence.
- Health endpoints: liveness `/healthz`, readiness `/readyz` (canonical, no `/api/readyz`). System gate at `/system/ready`.
- Migrations are signed (`migrations/signatures.json`); run `cargo sqlx prepare` after query changes.
- Runtime dirs: `var/logs` for boot/service logs, `var/run` for sockets/pids. Clean with `./scripts/fresh-build.sh` or manual removal if ports are stuck.

## Commit & Pull Request Guidelines
- Commit format: `type(scope): summary` (e.g., `fix(router): stabilize Q15 tie-break`). Include why, what changed, and links to issues/IDs.
- PRs: describe behavior and risk, list commands/tests run, note determinism impact, and attach UI visuals when applicable. Keep scope tight; prefer one-off scripts over new tooling. Ensure OpenAPI/TypeScript clients and docs stay in sync before merge.
