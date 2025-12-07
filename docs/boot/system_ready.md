# System Ready Signal and Boot Timing

## What is covered
- System Ready is true only when DB, adapteros-server boot state, workers (UDS), router, and UI health checks all report healthy.
- Readiness is exposed via HTTP and a local flag file for boot scripts.

## Signals and endpoints
- HTTP: `GET /system/ready` (200 when ready, 503 otherwise). Response includes per-component status and `boot_elapsed_ms`.
- Local flag file: default `var/run/system_ready` (override with `AOS_SYSTEM_READY_PATH`). Written as JSON with timestamp and component statuses when readiness first becomes true; removed if readiness fails.
- Boot log: default `var/log/boot-times.log` (override with `AOS_BOOT_LOG_PATH`). Appends `boot_ms` and timestamp on the first ready transition per boot.

## Inputs used for readiness
- Server boot lifecycle (`BootStateManager` readiness).
- Database connectivity and migration table presence.
- Router metrics snapshot (idle is treated as healthy).
- Worker health monitor summaries (all workers must be healthy; degraded/unknown/crashed blocks readiness).
- UI health endpoint at `AOS_UI_HEALTH_URL` (default `http://127.0.0.1:3200/healthz`).

## How to check
- CLI: `curl -f http://127.0.0.1:8080/system/ready | jq`.
- Local flag: `cat $(AOS_SYSTEM_READY_PATH:-var/run/system_ready)` for JSON payload.
- Logs: `tail -n 5 $(AOS_BOOT_LOG_PATH:-var/log/boot-times.log)` for boot durations.

## Notes
- File paths are created on demand; ensure the parent directories are writable by the service user.
- If UI health is unreachable or workers are degraded, readiness returns 503 and the flag is removed.

MLNavigator Inc December 6, 2025.

