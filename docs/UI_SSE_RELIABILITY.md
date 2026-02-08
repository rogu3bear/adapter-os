# UI SSE Reliability (Manual Test Recipe)

Goal: UI streaming updates must not silently die. On network blips, the UI should recover automatically and fall back to polling without hanging.

## Preconditions

- Run the system in dev mode (no auth gates):

```bash
AOS_DEV_NO_AUTH=1 ./start
```

## What To Test

### 1) Workers Stream (System Page)

1. Open `http://localhost:8080/system` (or your configured UI origin).
2. Confirm the SSE indicator shows `Live` and updates `last <Ns` continuously.
3. In browser DevTools:
   - Switch Network to `Offline` for ~15 seconds.
   - Switch back to `Online`.
4. Expected:
   - Indicator transitions to a degraded state (`(polling)`).
   - Worker list continues updating via polling while SSE is down.
   - On recovery, indicator returns to `Live` and polling stops.

### 2) Metrics Stream (Dashboard)

1. Open `http://localhost:8080/dashboard`.
2. Repeat the `Offline` / `Online` toggle.
3. Expected:
   - Indicator transitions to `(polling)` while disconnected.
   - Dashboard metrics continue updating via REST fallback polling.
   - On recovery, indicator returns to `Live`.

### 3) Client Errors Stream (Incidents)

1. Open `http://localhost:8080/errors` and keep the `Live Feed` tab open.
2. Toggle `Offline` / `Online`.
3. Expected:
   - Indicator transitions to `(polling)` while disconnected.
   - Live feed continues to show new errors (from polling) without duplicating IDs.
   - On recovery, indicator returns to `Connected`.

## Anti-Goals (Things That Must Not Happen)

- UI hangs forever in a “connecting” state without switching to polling.
- Unbounded reconnect loops that spam the network.
- SSE silently stalls (no last-event timestamp movement) without recovery.

