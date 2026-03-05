# Worker Crash

Worker process down, 503 errors. SEV-1.

---

## Symptoms

- Inference 503
- `ps aux | grep aos-worker` empty
- `var/run/worker.sock` (dev startup path) or `var/run/aos/<tenant>/worker.sock` (prod tenancy path) missing

**User-facing:** Chat "Service temporarily unavailable", API `SERVICE_UNAVAILABLE`, `WORKER_UNAVAILABLE`.

---

## Diagnosis

```bash
# Logs
tail -200 var/logs/*.log | grep -i "panic\|fatal\|error"

# OOM (Linux)
dmesg | grep -i "killed" | grep aos

# Socket
ls -la var/run/aos/*/worker.sock
```

**Code:** `WorkerHealthMonitor` polls workers; `get_worker_health_summary` reports status. Socket path: `adapteros-core::defaults::DEFAULT_WORKER_SOCKET_PROD_ROOT` = `var/run/aos`.

---

## Readiness Truth

Treat worker readiness as true only when all three are true:

1. PID is alive
2. Expected socket exists
3. Socket has a live listener

```bash
scripts/service-manager.sh status
if [ -f var/worker.pid ]; then cat var/worker.pid; fi
ls -la var/run/worker.sock
lsof -t var/run/worker.sock
tail -n 200 var/logs/service-manager.log
```

Control-plane `registered` is transitional and not a readiness guarantee.

---

## Resolution

```bash
# Manual restart
pkill -9 -f aos-worker
rm -f var/run/aos/*/worker.sock
./start worker

# Or full restart
./start down && ./start
```

**Service manager:** `scripts/service-manager.sh` can auto-restart worker if configured.

---

## Common Causes

| Cause | Fix |
|-------|-----|
| OOM | Reduce adapters, increase headroom; see [MEMORY_PRESSURE](MEMORY_PRESSURE.md) |
| Adapter corruption | Quarantine adapter; check `manifest_hash` |
| Backend init | Rebuild with correct features (`mlx-backend`) |
| Socket permission | `chmod` var/run/aos |
