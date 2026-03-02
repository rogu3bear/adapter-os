# Foundation Run

## Prerequisites

- Run from the repository root.
- Rust/Cargo installed (`rustup toolchain install stable`).
- `curl` available.
- `var/` writable (runtime files/logs stay under `var/`).
- For UI regeneration when static assets are missing/invalid: `trunk` (and optional `wasm-opt`) available for `scripts/build-ui.sh`.
- For clean-tree runtime verification, use a temporary worktree path outside `/tmp` (persistent runtime paths under `/tmp` are intentionally rejected by config validation).

## One Command

```bash
scripts/foundation-run.sh
```

## Flags

- `--full-clean`: run `scripts/fresh-build.sh --full-clean` before build/start.
- `--no-clean`: skip the clean step.
- `--headless`: skip `/` smoke check only (UI assets are still verified and regenerated if needed).
- `--strict-ready`: forward strict readiness checks to `scripts/foundation-smoke.sh` (`/readyz` must be 200 with db/worker/models checks all true).
- `--allow-dev-bypass`: only used with `--strict-ready`; allows `readiness_mode=dev_bypass`.
- `--workspace`: build full workspace instead of server-only (default: server-only).

## What It Does

1. Runs preflight checks (repo root, required tools/scripts, config path).
2. Optionally cleans runtime/build artifacts through `scripts/fresh-build.sh`.
3. Validates UI static assets and auto-regenerates with `scripts/build-ui.sh` when required.
4. Builds the server (`cargo build -p adapteros-server`, or `cargo build --workspace` with `--workspace`) against `DATABASE_URL=sqlite://target/sqlx-compile-schema.sqlite3` by default (auto-generated from `migrations/*.sql`, override via environment).
5. Ensures a repo-local model path exists for startup (`AOS_MODEL_PATH`, or `AOS_MODEL_CACHE_DIR` + `AOS_BASE_MODEL_ID`; falls back to `var/models/Qwen2.5-7B-Instruct-4bit`).
6. Starts the backend service (`aos-server`) with `configs/cp.toml` (or `AOS_CONFIG`).
7. Runs `scripts/foundation-smoke.sh` against that server.
   - Add `--strict-ready` when CI/runtime must require full worker+model readiness.
8. Prints local endpoints and artifact locations.
9. Stays attached; on exit/Ctrl-C it shuts down the owned backend process cleanly.

## What Success Looks Like

- `scripts/foundation-run.sh` prints `Stabilization run complete`.
- Smoke output ends with `[foundation-smoke] PASS`.
- Endpoints print for `/healthz` and `/readyz` (and `/` in non-headless); `/readyz` must keep status/body consistent (`200 => ready=true`, `503 => ready=false`).
- Backend log file exists at `var/logs/foundation-backend.log`.

## Logs and Outputs Under `var/`

- `var/logs/foundation-backend.log`: backend stdout/stderr for the run.
- `var/run/foundation-backend.pid`: pid for backend owned by foundation run.
- `var/logs/foundation-smoke-backend.log`: only when smoke auto-starts its own backend.
- `var/tmp/foundation-smoke.*`: temporary smoke response bodies.

## Common Failure Modes and Exact Fixes

| Failure | Exact fix |
| --- | --- |
| `Missing command: cargo` | Install Rust toolchain: `rustup toolchain install stable && rustup default stable` |
| `Config not found: ...` | Set `AOS_CONFIG` to a valid repo-relative config (for example `configs/cp.toml`) |
| UI assets invalid and auto-rebuild fails | Run `bash scripts/build-ui.sh` directly and fix the reported issue |
| `trunk` missing when UI assets must be regenerated | Install trunk (`cargo install trunk`) or provide valid static assets first |
| Backend already running on target port | Stop the existing process, then rerun foundation run |
| Build failure | Fix compile errors from `cargo build -p adapteros-server` (or `--workspace`), then rerun |
| Smoke failure | Inspect `var/logs/foundation-backend.log` (or `var/logs/foundation-smoke-backend.log`) and rerun smoke |

## Static Assets Expectation (Explicit)

- `--full-clean` wipes build artifacts and static asset contents but preserves `crates/adapteros-server/static/` itself.
- `scripts/foundation-run.sh` always verifies static assets and auto-regenerates them before the build step.
- `--headless` skips only the `/` smoke path check.
