# Demo Mode (Environment Overlay)

`configs/demo.env` is a small, sourceable environment overlay for running repeatable local demos. It pins ports, a dedicated SQLite DB file, logging verbosity, and a few common timeouts without changing runtime logic or code.

It intentionally does **not** set model/backend/security variables; keep those in your `.env` / `.env.local` (see `.env.example`).

## What It Pins

- **Ports**
  - `AOS_SERVER_HOST=127.0.0.1`
  - `AOS_SERVER_PORT=8080`
  - `AOS_UI_PORT=3200`
  - `AOS_PANEL_PORT=3301`
- **DB path**
  - `AOS_DATABASE_URL=sqlite:var/aos-demo.sqlite3`
- **Log levels**
  - `AOS_LOG_LEVEL=info`
  - `RUST_LOG=info,adapteros=debug`
- **Timeouts**
  - `AOS_DATABASE_TIMEOUT=30s`
  - `AOS_WORKER_SHUTDOWN_TIMEOUT=30s`
  - `AOS_DOWNLOAD_TIMEOUT_SECS=300`

## Use It (Manual)

1. Ensure your normal env is set up (model path, etc.). If you don’t already have one:
   - `cp .env.example .env` and edit as needed.

2. Load the demo overlay into your shell:

```bash
set -a
source configs/demo.env
set +a
```

3. Run the server and UI:

```bash
make dev
make ui-dev
```

Endpoints:
- API: `http://127.0.0.1:8080` (health: `http://127.0.0.1:8080/healthz`)
- UI: `http://127.0.0.1:3200`

## Use It With `direnv` (Recommended)

This repo includes a `.envrc` that sources `.env` and `.env.local`. To apply demo settings automatically:

1. Add to `.env.local`:

```bash
source configs/demo.env
```

2. Reload:

```bash
direnv allow
```

## Reset Demo State

- Stop the server and delete the demo DB file: `rm -f var/aos-demo.sqlite3`
