# Drift Log

Last updated: 2026-02-16

## Scope

Targeted autopilot triage focused on active churn areas (`adapteros-ui`, `adapteros-server-api`).

## Verified Health

- `cargo check -p adapteros-ui` passes.
- `cargo check -p adapteros-ui --target wasm32-unknown-unknown` passes.
- `cargo check -p adapteros-server-api` passes.
- `cargo check -p adapteros-ui --all-targets` passes.
- `cargo check -p adapteros-server-api --tests` passes.
- `cargo check -p adapteros-server` passes.
- `cd tests/playwright && npx playwright test ui/routes.ops.smoke.spec.ts --list` passes.
- `cargo test -p adapteros-server-api --test api_contracts -- --nocapture --test-threads=1` passes.

## Resolved Drift

1. `docs/API_REFERENCE.md` KV-isolation route prefix mismatch
   - Previous docs paths:
     - `GET /v1/kv-isolation/health`
     - `POST /v1/kv-isolation/scan`
   - Verified canonical paths (router + OpenAPI):
     - `/v1/storage/kv-isolation/health`
     - `/v1/storage/kv-isolation/scan`
   - Patch applied in `docs/API_REFERENCE.md` to align with router and OpenAPI.

2. Playwright smoke route coverage gap for `/files`
   - Route exists in Leptos router: `crates/adapteros-ui/src/lib.rs`.
   - Route was missing from hard-coded smoke arrays.
   - Patch applied in `tests/playwright/ui/routes.ops.smoke.spec.ts`:
     - Added `{ path: '/files', heading: 'Files' }`.
   - Verified via Playwright test discovery (`--list`) showing `/files` smoke test entry.

3. `testkit/reset` connection-pool starvation during contract tests
   - Failure observed in `api_contract_snapshots`: `Migration failed: while executing migrations: pool timed out while waiting for an open connection`.
   - Root cause in `crates/adapteros-server-api/src/handlers/testkit.rs`:
     - Reset handler retained an acquired DB connection while calling `state.db.migrate()`.
   - Patch applied:
     - Explicitly `drop(conn)` before `state.db.migrate()`.
   - Result:
     - Timeout failure no longer reproduces in `api_contracts` test target.

4. Snapshot harness drift for `api_contracts` compatibility target
   - `tests/api_contracts.rs` includes `contract_snapshots.rs`, which changed module-path-derived snapshot prefixes (`api_contracts__*` vs existing `contract_snapshots__*` baselines).
   - Patch applied in `crates/adapteros-server-api/tests/contract_snapshots.rs`:
     - `settings.set_prepend_module_to_snapshot(false)` to avoid module-path coupling.
     - Added `assert_contract_snapshot!` macro with stable `contract_snapshots__` prefix.
     - Switched snapshot assertions to the stable macro.
   - Then refreshed snapshots for actual contract changes with:
     - `INSTA_UPDATE=always cargo test -p adapteros-server-api --test api_contracts api_contract_snapshots -- --nocapture --test-threads=1`
   - Updated snapshots:
     - `contract_snapshots__healthz_contract.snap`
     - `contract_snapshots__tenants_forbidden_contract.snap`
     - `contract_snapshots__auth_login_failure.snap`
     - `contract_snapshots__inference_stub_contract.snap`

5. Chat UI closure fragility hardening (`crates/adapteros-ui/src/pages/chat.rs`)
   - Risk areas previously logged:
     - Non-`move` `Show` fallbacks at `/chat`, `/chat/:session_id`, and workspace empty-state.
     - Local `select_last_n` closure reused across multiple quick-select buttons.
   - Patch applied:
     - Converted `fallback=|| ...` to `fallback=move || ...` at those call sites.
     - Converted quick-select handler to clone-safe `Callback` and used per-button clones.
   - Verified with:
     - `cargo check -p adapteros-ui`

## Potential Drift (Unconfirmed)

- None currently tracked from this pass.

## Decision Log

- No autopatch applied for unconfirmed drift items because there is no active failure and other agents are concurrently editing adjacent UI code.
- Keep this file as the running ledger for uncertain drift signals during multi-agent work.
- `auth_enhanced` contract tests vs routes/OpenAPI: no evidence-backed drift found in this pass.
