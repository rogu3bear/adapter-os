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
```

Notes:
- Configs default to repo-local `var/playwright` if these env vars are unset.

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
```

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

### Full Suite (default)

```bash
npm run test:ui
```

Starts:
- Backend: `E2E_MODE=1 AOS_DEV_NO_AUTH=0 AOS_DATABASE_URL=var/playwright/aos-cp.sqlite3 AOS_KV_PATH=var/playwright/aos-kv.redb AOS_KV_TANTIVY_PATH=var/playwright/aos-kv-index AOS_MODEL_CACHE_DIR=var/playwright/models AOS_BASE_MODEL_ID=mistral-7b-instruct-v0.3-4bit AOS_DEV_JWT_SECRET=dev-secret AOS_SKIP_PREFLIGHT=1 AOS_RATE_LIMITS_REQUESTS_PER_MINUTE=10000 AOS_RATE_LIMITS_BURST_SIZE=2000 AOS_RATE_LIMITS_INFERENCE_PER_MINUTE=10000 target/debug/aos-server --config configs/cp.toml`
- UI: Served by the adapteros-server embedded assets at `http://localhost:8080`

Notes:
- The UI suite clears `var/playwright/aos-cp.sqlite3`, `var/playwright/aos-kv.redb`, and `var/playwright/aos-kv-index` before starting the backend.
- The backend process is kept alive by the Playwright webServer wrapper and is stopped on exit.
 - Tests use `http://localhost:8080` to avoid cross-origin API calls.

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
```

## Cleanup

```bash
rm -rf var/playwright/test-results var/playwright/report var/playwright/tmp
```
