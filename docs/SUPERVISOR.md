# Supervisor, Service Control, And Safe Restart

This document is the canonical reference for how adapterOS integrates with an external
**supervisor** (process/service manager) and how that affects:

- Service Control endpoints (`/v1/services/*`)
- Lifecycle endpoints (`/admin/lifecycle/*`, especially safe restart)

## When You Need A Supervisor

Running `./start` in local development mode starts processes directly (via `scripts/service-manager.sh`)
and does **not** configure an external supervisor by default.

A supervisor is required when you want HTTP-driven service orchestration such as:

- Starting/stopping services through `/v1/services/*`
- Using `/admin/lifecycle/safe-restart` to safely exit and be restarted automatically

Supervisor orchestration manifests live under:

- `deploy/supervisor.yaml` (general orchestration mode)
- `deploy/supervisor.local.yaml` (local-dev port roles that match `docs/PORTS.md`)

## Supervisor Configuration

The control plane discovers the supervisor via either:

- `SUPERVISOR_API_URL` (preferred)
- `AOS_PANEL_PORT` (constructs `http://127.0.0.1:${AOS_PANEL_PORT}`)

If neither is set, the supervisor-dependent endpoints return:

- HTTP `503`
- error code `SUPERVISOR_NOT_CONFIGURED`

This is expected when running `./start` without a supervisor.

## Service Control API (`/v1/services/*`)

These endpoints proxy requests to the supervisor API and require `NodeManage` permission.

If the supervisor is not configured, the API returns `503 SUPERVISOR_NOT_CONFIGURED` with
remediation to set `SUPERVISOR_API_URL` or `AOS_PANEL_PORT`.

## Lifecycle Endpoints (`/admin/lifecycle/*`)

### Maintenance (`POST /admin/lifecycle/request-maintenance`)

Sets the system to maintenance mode (reject new work; allow in-flight to continue).

### Shutdown (`POST /admin/lifecycle/request-shutdown`)

Transitions the system to draining and then:

- `mode=drain`: drains in-flight requests and exits
- `mode=immediate`: drains and then stops without waiting for in-flight completion

### Safe Restart (`POST /admin/lifecycle/safe-restart`)

Safe restart is intended for supervised environments:

1. Sets maintenance mode
2. Transitions to draining
3. Triggers an in-process graceful shutdown (drain + bounded timeout)
4. Exits so an external supervisor can restart the process

If the supervisor is not configured, the endpoint returns:

- HTTP `503`
- error code `SUPERVISOR_NOT_CONFIGURED`

This is a safety guard to avoid shutting down a process that will not be restarted.

#### Override (Use With Caution)

If you explicitly want safe-restart to shut down even without a supervisor, set:

- `AOS_ALLOW_UNSUPERVISED_SAFE_RESTART=1`

This is intended for controlled environments where an operator will restart the process manually.

