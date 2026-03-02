# Playwright UI Tests

Local-only Playwright suites for AdapterOS UI surfaces.

## Requirements

- Node.js + npm
- Playwright browsers installed for this repo
- Backend + UI run locally (see below)

## Setup

```bash
cd tests/playwright
npm install
npx playwright install
```

## Environment (required for artifacts)

```bash
export PLAYWRIGHT_BROWSERS_PATH=$(cd ../.. && pwd)/var/playwright/browsers
export PW_TEST_TMPDIR=$(cd ../.. && pwd)/var/playwright/tmp
# Optional run overrides
export PW_RUN_ID=local-ui
export PW_SERVER_PORT=4180
export PW_CLEAN_DEBUG=0
```

Notes:
- Configs default to repo-local `var/playwright` if these env vars are unset.
- `PW_RUN_ID`: run scope key; if unset, `scripts/pw-run.mjs` generates a unique `run-...` ID.
- `PW_SERVER_PORT`: backend/UI port for this run; if unset, `scripts/pw-run.mjs` picks an available port.
- `PW_CLEAN_DEBUG`: optional teardown cleanup toggle; set to `1` to remove `<run-id>/debug`, otherwise debug artifacts are preserved.

Run-scoped artifacts (Leptos UI configs):
- Root: `var/playwright/runs/<run-id>/`
- Test output: `var/playwright/runs/<run-id>/test-results/`
- HTML report: `var/playwright/runs/<run-id>/report/`
- Temp dir: `var/playwright/runs/<run-id>/tmp/`
- Backend state: `var/playwright/runs/<run-id>/aos-cp.sqlite3`, `aos-kv.redb`, `aos-kv-index/`
- Diagnostics: `var/playwright/runs/<run-id>/heartbeat.json`, `debug/global-setup.ndjson` (plus `debug/global-setup-dashboard.png` on setup failure)

## Run Suites

### Leptos UI

```bash
# Full suite (chromium + webkit, serial)
npm run test:ui

# Fast local dev (chromium only, 4 workers, no traces)
npm run test:ui:fast

# Single browser
npm run test:ui:chrome

# Specific test files
npm run test:smoke      # Route smoke tests only
npm run test:visual     # Visual regression only
npm run test:audit      # Route/accessibility audit surfaces
npm run test:gate:quality -- --project=chromium
npm run test:gate:quality -- --project=webkit
```

### Blocking UI Quality Gate

The canonical blocking gate is `test:gate:quality` and bundles:

- `ui/console.regression.spec.ts`
- `ui/routes.best_practices.audit.spec.ts`
- `ui/visual.spec.ts`
- `ui/runs.spec.ts`

Before running those specs, the gate enforces visual baseline contract checks via:

```bash
node scripts/check-visual-snapshot-contract.mjs
```

Baseline policy:
- Canonical baselines are macOS (`*-darwin.png`) for Chromium and WebKit.
- Active visual assertions must have both browser baselines present.
- Snapshot files without matching `toHaveScreenshot(...)` references are treated as contract violations.

### Speed Tips

| Command | Time | Use Case |
|---------|------|----------|
| `npm run test:ui:fast` | ~2-3min | Local dev iteration |
| `npm run test:ui:chrome` | ~5min | Single browser, full features |
| `npm run test:ui` | ~10min | Full CI validation |

**Run single spec:**
```bash
npx playwright test -c playwright.fast.config.ts auth.spec
```

**Run with headed browser (debugging):**
```bash
npx playwright test -c playwright.fast.config.ts --headed auth.spec
```

### CI Sharding

Split tests across 3 parallel CI jobs:
```bash
npm run test:ci:shard1  # Job 1
npm run test:ci:shard2  # Job 2
npm run test:ci:shard3  # Job 3
```

### CI Concurrency Smoke (advisory)

```bash
npm run test:ci:concurrency
```

Runs two Leptos slices concurrently via `scripts/pw-concurrency-smoke.mjs`:
- `concurrency-a`: `chromium`, `PW_RUN_ID=ci-concurrency-a`, `PW_SERVER_PORT=4190`, specs `ui/auth.spec.ts` + `ui/routes.core.smoke.spec.ts`
- `concurrency-b`: `webkit`, `PW_RUN_ID=ci-concurrency-b`, `PW_SERVER_PORT=4191`, specs `ui/auth.spec.ts` + `ui/routes.core.smoke.spec.ts`

In CI (`.github/workflows/ci.yml`), this job is advisory (`continue-on-error: true`) and always uploads:
- `playwright-concurrency-a` from `var/playwright/runs/ci-concurrency-a/`
- `playwright-concurrency-b` from `var/playwright/runs/ci-concurrency-b/`

### Full Suite (default)

```bash
npm run test:ui
```

Starts:
- Backend: `E2E_MODE=1` server with run-scoped DB/KV state under `var/playwright/runs/<run-id>/...`
- UI: Served by adapteros-server embedded assets at `http://localhost:<PW_SERVER_PORT>`

Notes:
- The UI suite clears `var/playwright/runs/<run-id>/aos-cp.sqlite3`, `aos-kv.redb`, and `aos-kv-index/` before starting the backend.
- The backend process is kept alive by the Playwright webServer wrapper and is stopped on exit.
- Tests use `http://localhost:<PW_SERVER_PORT>` to avoid cross-origin API calls.

### CodeGraph Viewer

```bash
npm run test:codegraph
```

Starts:
- Vite dev server: port `5173`
- Tests use `VITE_CODEGRAPH_TEST_DATA=1` and `/?testData=1` to load fixture data.

### Static Minimal

```bash
npm run test:minimal
```

Starts:
- Static server: port `3210`

## Reports

```bash
npm run report
# Run-scoped report (recommended)
npx playwright show-report var/playwright/runs/<run-id>/report
```

## Cleanup

```bash
rm -rf var/playwright/runs/<run-id>
```
