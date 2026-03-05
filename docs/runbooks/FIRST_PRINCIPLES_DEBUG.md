# First-Principles Debugging

Systematic root-cause method for incidents where logs, status, and behavior disagree.

---

## When to Use

- Startup says "ready" but runtime behavior fails
- Multiple components report conflicting health states
- Retries/restarts hide the real failure boundary
- You need a minimal fix with high confidence

---

## Method (Mandatory Loop)

1. Define success in physical terms
   - Example: "worker is usable only when process is alive, UDS socket exists, and requests succeed."
2. Write necessary conditions (not assumptions)
   - Process/pid state
   - Socket/network listener state
   - Dependency state (DB, model, manifest, config)
   - Control-plane lifecycle state
3. Build the explicit state machine
   - Starting -> registered -> socket-bound -> healthy/serving
4. Collect evidence in order at each boundary
   - Stop at the first contradiction.
5. Remove the false assumption at the boundary where it enters
   - Prefer one small gate/condition change over broad refactors.
6. Re-run the original failing path end-to-end
   - Prove the old failure is gone with timestamps/log evidence.

---

## AdapterOS Command Set

```bash
# 1) Snapshot current state
./start status
scripts/service-manager.sh status
pgrep -fl "aos-server|aos-worker|service-manager.sh"

# 2) Ground truth for worker readiness
ls -la var/run/worker.sock
lsof -t var/run/worker.sock
if [ -f var/worker.pid ]; then cat var/worker.pid; fi

# 3) Lifecycle evidence
tail -n 200 var/logs/start.log
tail -n 200 var/logs/service-manager.log
tail -n 200 var/logs/worker.log

# 4) Control-plane status check (if DB exists)
sqlite3 var/aos-cp.sqlite3 \
  "SELECT pid,status,last_seen_at FROM workers ORDER BY last_seen_at DESC LIMIT 10;"

# 5) Reproduce the exact path
./start down && ./start
```

---

## Evidence Template

- Incident ID / run timestamp:
- Expected success condition:
- First contradiction observed:
- False assumption identified:
- Minimal boundary fix:
- Validation command(s):
- Before/after log lines with timestamps:
- Residual risk:

---

## Resolution Rules

- Patch the earliest incorrect gate, not downstream symptoms.
- Do not claim fixed until the original failing path passes.
- Keep changes minimal and add a regression check when possible.

---

## Reconciliation Note (2026-03-03)

- False assumption removed: `registered == ready`.
- Boundary fixed: `scripts/service-manager.sh` worker readiness gate now requires socket truth before success.
- Baseline contradiction:
  - `2026-03-03T05:29:16-06:00`: service manager reported `Worker started ... Control Plane: registered`
  - `2026-03-03T05:29:31-06:00`: start reported `worker:socket_timeout timeout=15s`
- Post-fix evidence:
  - `2026-03-03T05:38:51-06:00`: service manager reported `Worker started ... Socket: /Users/star/Dev/adapter-os/var/run/worker.sock`
  - `2026-03-03T05:38:55-06:00`: start reported `startup:complete backend=ok worker=1`
