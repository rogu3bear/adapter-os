# Ports (Source Of Truth)

This document defines the canonical port roles and environment variables for AdapterOS.
If you see conflicting defaults elsewhere in the repo, treat this file as the contract.

## Canonical Port Roles

| Role | Env var | Default | Purpose |
| --- | --- | ---: | --- |
| Control plane HTTP (API + embedded UI assets) | `AOS_SERVER_PORT` | 8080 | Primary HTTP server: `/v1/*`, `/healthz`, `/readyz`, and the embedded Leptos UI assets when built into `crates/adapteros-server/static`. |
| Leptos UI dev server (Trunk) | `AOS_UI_PORT` | 3200 | Trunk dev server for UI iteration. Proxies API calls to the control plane. |
| Service supervisor panel API | `AOS_PANEL_PORT` | 3301 | Service control API used by `/v1/services/*` when a supervisor is configured. |
| Node agent HTTP (dev mode) | `AOS_NODE_PORT` | 9443 | Node agent service (TCP in dev mode; UDS in production mode). Stored in `nodes.agent_endpoint`. |

## Precedence (How Ports Are Chosen)

When a tool supports explicit overrides, the port selection order is:

1. Tool-specific env (example: `PW_BASE_URL` for Playwright)
2. Role env vars above (`AOS_*_PORT`, or `AOS_SERVER_URL` where supported)
3. Fallback numeric defaults (the table above)

## Local Dev Modes

### Mode A: `./start` (default)

- Control plane server listens on `http://localhost:${AOS_SERVER_PORT:-8080}`.
- The control plane serves the Leptos UI from embedded assets (built via Trunk into `crates/adapteros-server/static`).

### Mode B: Trunk dev server (UI iteration)

Run the UI dev server:

```bash
# Preferred: respects AOS_UI_PORT and proxies to AOS_SERVER_URL / AOS_SERVER_PORT.
./scripts/ui-dev.sh
```

Notes:
- `crates/adapteros-ui/Trunk.toml` contains a fixed proxy target (`http://localhost:8080`) because Trunk does not support env interpolation inside the TOML.
- `./scripts/ui-dev.sh` generates a Trunk config under `./var/` with the correct proxy target so "port groups" work (for example `AOS_SERVER_PORT=8180`).

Trunk proxies API paths to the control plane, so you typically run:

- UI: `http://localhost:${AOS_UI_PORT:-3200}`
- API: `http://localhost:${AOS_SERVER_PORT:-8080}`

## Cluster / Worker Examples (Not Local Defaults)

Some docs use ports like `8081`, `8082`, ... for *additional nodes*.
Treat those as **node-specific server ports** for a cluster deployment, not as the canonical local defaults.

## Supervisor Orchestration Mode

`deploy/supervisor.yaml` is a separate orchestration mode and may use a different port layout
(for example backend on `3300`, panel on `3301`, and a Trunk UI service on `8080`).
Do not treat that file as the canonical local-dev default.

If you want a supervisor stack that matches the canonical local port roles, use `deploy/supervisor.local.yaml`.

Note: `/v1/services/*` and `/admin/lifecycle/safe-restart` assume a supervisor is configured. When running `./start` without a supervisor, these endpoints return 503 `SUPERVISOR_NOT_CONFIGURED` unless explicitly overridden. See [SUPERVISOR.md](SUPERVISOR.md).

## Port IO Error Handling Contract

This section defines required behavior for any AdapterOS code that **binds** a port (servers)
or **talks to** a port (HTTP clients / scripts / tests).

### Requirements

- **Binders (servers)**
  - Must fail fast on bind errors.
  - Must special-case `AddrInUse` with:
    - the port/addr attempted
    - the relevant env var name to change (`AOS_SERVER_PORT`, `AOS_UI_PORT`, `AOS_PANEL_PORT`, `AOS_NODE_PORT`)
    - one remediation command: `lsof -nP -i :$PORT -sTCP:LISTEN`

- **Consumers (scripts/CLI/tests)**
  - Must never hang: every HTTP call must set connect timeout + total timeout.
  - Must not treat non-2xx as success unless explicitly best-effort.
  - Must include enough context on failure: method, URL, HTTP code, and first 300 chars of body.

- **Best-effort probes (status/diagnose)**
  - Allowed to continue, but must:
    - use short timeouts
    - log warning on failure

### Concrete Before/After Fixes (Anchored)

| Area | Before | After |
| --- | --- | --- |
| `scripts/service-manager.sh` | Missing `scripts/port-guard.sh` silently disabled port checks (`ensure_port_free(){ return 0; }`). | Missing port guard now fails starts with a clear error to avoid confusing downstream failures. |
| `scripts/metrics-bridge.sh` | Push used `curl -s` with no connect/total timeout; could hang. | Push uses connect + total timeout and fails fast per-iteration with warnings. |
| `scripts/test/quick_inference_test.sh` | No strict mode; `curl -s` without status checking. | Strict mode + bounded curl calls + explicit skip messaging when auth fails. |
| `scripts/start_minimal_training.sh` | Repo list used `curl -s` and assumed JSON; failures produced opaque parse errors. | Uses `scripts/lib/http.sh` helpers, checks HTTP codes, and prints response snippets on failures. |
| CLI HTTP (reqwest) | Many commands used `reqwest::Client::new()` (no timeout). | CLI builds clients with a hard timeout so commands cannot hang indefinitely. |
| TCP bind errors | Some bind errors surfaced raw `AddrInUse` without remediation. | Bind errors include port role/env var hints and a concrete `lsof` remediation command. |
