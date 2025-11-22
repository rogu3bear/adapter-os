# Cypress Lifecycle Testing Plan

## Goals
- Automate the documented system startup steps (preflight checks, migrations, tenant/model imports, server launch, and readiness/metrics probes) so the “heavy” boot choreography in `System Startup and Shutdown` is guarded by a repeatable test flow rather than a manual runbook entry.【source: docs/OPERATIONAL_RUNBOOKS.md L20-L112】
- Capture the Persona Journey Dashboard experience—login, persona slider, stage progression, responsive layout, and accessibility hooks described in the guide—so UI regressions that affect multiple personas or widgets get surfaced quickly once the backend stack is running.【source: docs/PERSONA_DEMO_USER_GUIDE.md L9-L198】

## Requirements
1. **Lifecycle orchestration**: Cypress should launch the supporting services (mocked DB or dedicated test DB, `aosctl` migrations/tenant/model bootstrapping, then `adapteros-server`) before touching health/UI endpoints, and tear them down after the spec finishes so each run mirrors the real lifecycle.
2. **Readiness validation**: The spec must probe `/healthz`, `/readyz`, and `/metrics` to verify server readiness (per the runbook checklist) before moving on to UI interactions, ensuring server-side regressions fail fast.
3. **Persona UX**: Once ready, the suite should log in with a seeded admin/tenant, land on the Dashboard, and interact with the persona slider/play stage controls described in the Persona Journey guide—this keeps the rich, multi-role UX path covered without depending on API-only tests.

## Incremental Implementation Steps
1. **Tooling scaffolding**: Introduce a Cypress project under `ui/e2e` (or similar) with `cypress.config.ts`, fixture support for fixtures/seeds, and npm scripts (`pnpm cypress:open`, `pnpm cypress:run`). Keep the UI build toolchain intact and reference the existing `ui` package manifest.
2. **Lifecycle helpers**: Add custom commands (`startServer`, `stopServer`, `seedTenant`, `resetDb`) that run the `aosctl` commands in `docs/OPERATIONAL_RUNBOOKS.md` (migrate, init-tenant, import-model) and manage the server process. These helpers should be durable so future specs reuse them.
3. **First spec (boot + dashboard)**: Write a spec that:
   - Kicks off the lifecycle helpers, waits for `/healthz`/`/readyz`/`/metrics`, and asserts healthy responses.
   - Logs in through the real UI, opens the Dashboard, and manipulates the persona slider, stage controls, and a widget (e.g., System Health) according to the Persona Journey guide’s described behavior.
   - Runs teardown steps to stop the server, truncate the test DB, and clear any imported adapters.
4. **CI integration & documentation**: Document how to execute the suite (e.g., `pnpm cypress:run`) and schedule it in CI (preferably gated on `main`), ensuring duplication prevention rules are respected when adding helpers that multiple specs will share.【source: docs/DUPLICATION_PREVENTION_GUIDE.md L1-L48】
5. **Expansion plan**: Outline future specs for keyboard navigation, quick actions, adapter registration, and lifecycle error handling; reuse the lifecycle helpers so each new test benefits from the same startup/teardown flow, and refer back to the Persona Journey doc for each persona’s expected controls.

## Citations
- Startup/runbook: `docs/OPERATIONAL_RUNBOOKS.md` L20-L112 for boot sequence and verification steps.
- Persona UX: `docs/PERSONA_DEMO_USER_GUIDE.md` L9-L198 for dashboard controls and navigation expectations.
- Duplication guidance: `docs/DUPLICATION_PREVENTION_GUIDE.md` L1-L48 for extracting shared helpers.

## Execution & Cleanup
- **Start dependencies:** Run the backend per the runbook (`./target/release/adapteros-server --config configs/production.toml`, or the appropriate dev config) so `/healthz`, `/readyz`, `/metrics` respond before kicking off the Cypress suite; keep the UI dev server running on `http://localhost:3200` (`pnpm dev`).【source: docs/OPERATIONAL_RUNBOOKS.md L20-L112】
- **Run the suite:** From `ui/`, execute `pnpm cypress:run` (or `pnpm cypress:open` for exploratory work). Override `API_BASE_URL` if the server listens on a different port (`API_BASE_URL=http://localhost:8080 pnpm cypress:run`). The config file already targets `http://localhost:3200` as the default UI host.
- **Cleanup:** Stop the backend process after the tests finish and revert any temporary adapters/sessions per the runbook’s rollback steps (`kill $(cat /var/run/adapteros.pid)` and inspect logs). The Cypress spec itself assumes the database state is stable; extend future helpers (seed/teardown) with `aosctl` commands as needed while citing `docs/DUPLICATION_PREVENTION_GUIDE.md` when sharing utilities.
