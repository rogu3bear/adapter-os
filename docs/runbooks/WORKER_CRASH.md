# Worker Crash

Worker process down, 503 errors. SEV-1.

---

## Symptoms

- Inference 503
- `ps aux | grep aos-worker` empty
- `var/run/aos/<tenant>/worker.sock` missing

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
