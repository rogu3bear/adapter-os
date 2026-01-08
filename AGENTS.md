# Repository Guidelines

## Project Structure & Module Organization
- Rust workspace under `crates/`: `adapteros-core` (seed, determinism), `adapteros-server` (control plane), `adapteros-lora-*` (router, worker, kernels), `adapteros-ui` (Leptos WASM), `adapteros-tui` (terminal dashboard). Tools and helper binaries in `xtask/`, `aosctl`, and `tools/`.
- Frontend lives in `crates/adapteros-ui` (Trunk build, Rust-first types via `adapteros-api-types`); built WASM is served from `static/` by the backend. Terminal UI is in `crates/adapteros-tui` (ratatui + crossterm).
- Config/infrastructure: `configs/`, `config/`, `scripts/` (CI, build, env setup), `metal/` (kernels), `migrations/` (DB), shared fixtures in `test_data/`, integration tests in `tests/`, documentation in `docs/`. Runtime files land in `var/` (logs, pids, sockets).
- Reference docs to skim: `README.md`, `QUICKSTART.md`, `CONTRIBUTING.md`, `DETERMINISM_LOOP_IMPLEMENTATION_SUMMARY.md`, `docs/API_TYPE_GENERATION.md`, and `scripts/ci/check_openapi_drift.sh` usage.

## Entry Points: ./start, CLI, and TUI
- `./start up` (default): primary orchestrator; runs bannered preflight, backend via `scripts/service-manager.sh`, optional worker, readiness probe (`/readyz`), and offers `aosctl` install if missing. Reads `.env` (non-overriding) and writes logs to `var/logs/start.log`. `./start down` stops services cleanly; `./start status` queries service-manager.
- Flags: `--skip-worker`, `--quick` (skip readyz wait), `--worker-model <path>`, `--worker-tokenizer <path>`, `--worker-backend <mlx|coreml|metal>`, `--verbose`, `--trace`. Set `AOS_DEV_NO_AUTH=1` or run `make dev-no-auth` for UI exploration without touching RBAC code.
- CLI: build with `make cli` (symlinked `./aosctl`); commands like `aosctl models seed --model-path <dir>` and `aosctl tui` for the terminal dashboard when built with `--features tui`.
- TUI: `cargo run -p adapteros-tui` (ratatui) for live services/logs/config. Quick keys: `b` boot, `s` services, `l` logs, `m` metrics, `c` config, `q` quit. Uses REST on server port and metrics endpoints; falls back to mock data if offline.

## Local Dev, Build, and Run
- Backend dev: `make dev` (auth on) or `make dev-no-auth` / `AOS_DEV_NO_AUTH=1 ./start`. UI dev: `make ui-dev` (Trunk) and `make ui` for release WASM served from `static/`.
- Cargo-first workflow: prefer direct `cargo`/`./start` commands; Makefile is a convenience shim and may lag, so keep it in sync when commands change.
- Primary environment: macOS on Apple Silicon (M4 Max); we intend to go fully Mac‑first, so favor macOS tooling/assumptions unless explicitly asked to keep cross‑platform.
- Fresh builds: `make build` (release, metadata strip, env record), `make prepare` (clean ports), `cargo build --release --locked --offline` when isolating network. Metal shaders: `make metal`. CLI-only: `make cli`.
- Targeted iteration: `cargo check -p <crate>`, `cargo sqlx prepare` for DB query changes. Set `AOS_VAR_DIR` to redirect runtime outputs, `AOS_LOG_PROFILE=trace` for verbose boot logging.

## Testing & Quality Gates
- Suites: `make test` (fmt, clippy, Rust + Leptos), `make test-rust`, `make test-ui`, `make test-ignored` (tracking IDs required), `make test-hw` (macOS + Metal GPU). Debug with `cargo test -- --nocapture` or `--test-threads=1`.
- Determinism: `make determinism-check` whenever touching routing, seeds, kernels, or math. Use `AOS_DEBUG_DETERMINISM=1` to trace seed inputs and router tie-breaks.
- OpenAPI drift: `./scripts/ci/check_openapi_drift.sh --fix` and commit `docs/api/openapi.json` after API surface changes. Keep generated clients aligned (TypeScript client at `ui/src/api/generated.ts` when present; Leptos UI uses shared Rust types).
- Formatting/lint: `cargo fmt --all`, `cargo clippy --workspace -D warnings`. `make check` runs fmt, clippy, tests, determinism, SBOM.

## Coding Style, Determinism, and Naming
- Rust naming (`snake_case` items, `CamelCase` types); prefer `Result` for fallible flows; use `tracing` over `println!`. Keep Leptos components small; reuse `components/` and `hooks/`.
- Determinism invariants: HKDF-SHA256 with BLAKE3 global seed (`crates/adapteros-core/src/seed.rs`); router sorts score DESC with index ASC tie-break, Q15 denominator `32767.0` (`crates/adapteros-lora-router/src/constants.rs`); forbid `-ffast-math` flags. Maintain reproducible Metal builds (strip timestamps via `make build`).

## UI Workflow (Current Focus)
- Goal: get the Leptos UI visible and iterate on visuals; avoid backend/CI changes unless required. Use `make dev-no-auth` to explore pages without altering RBAC logic.
- Shared API types come from Rust crates; avoid ad-hoc JSON types. For new UI state, prefer signals/hooks; keep components deterministic and side-effect free.
- Add screenshots/gifs for UI PRs; ensure `make ui` passes for release artifacts.

## Security & Configuration Tips
- Secrets: never commit; use platform keychain providers. Follow `WORKER_SETUP.md` and `SECURITY.md` for hardened paths. `.env` is optional and non-overriding; export vars before `./start` to take precedence.
- Health endpoints: liveness `/healthz`, readiness `/readyz` (canonical, no `/api/readyz`). System gate at `/system/ready`.
- Migrations are signed (`migrations/signatures.json`); run `cargo sqlx prepare` after query changes.
- Runtime dirs: `var/logs` for boot/service logs, `var/run` for sockets/pids. Clean with `make prepare` or manual removal if ports are stuck.

## Commit & Pull Request Guidelines
- Commit format: `type(scope): summary` (e.g., `fix(router): stabilize Q15 tie-break`). Include why, what changed, and links to issues/IDs.
- PRs: describe behavior and risk, list commands/tests run, note determinism impact, and attach UI visuals when applicable. Keep scope tight; prefer one-off scripts over new tooling. Ensure OpenAPI/TypeScript clients and docs stay in sync before merge.
