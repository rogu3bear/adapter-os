# Runbooks

Incident response procedures.

---

## Runbooks

| Runbook | Scenario |
|---------|----------|
| [WORKER_CRASH](WORKER_CRASH.md) | Worker process down, 503 errors |
| [DETERMINISM_VIOLATION](DETERMINISM_VIOLATION.md) | Replay mismatch, hash divergence |
| [INFERENCE_LATENCY_SPIKE](INFERENCE_LATENCY_SPIKE.md) | P99 latency elevated |
| [MEMORY_PRESSURE](MEMORY_PRESSURE.md) | High memory, eviction failures |
| [DISK_FULL](DISK_FULL.md) | Disk exhausted, write failures |
| [ACTION_LOGS](ACTION_LOGS.md) | Local control-plane action log paths, retention, and UDS tail usage |
| [FIRST_PRINCIPLES_DEBUG](FIRST_PRINCIPLES_DEBUG.md) | Contradictory health/readiness signals, root-cause isolation |
| [QUANTIZE_QWEN35_RELEASE](QUANTIZE_QWEN35_RELEASE.md) | Deterministic Qwen3.5-27B quantization release and rollback |

---

## Quick Commands

```bash
curl -f http://localhost:18080/healthz
./aosctl doctor
./aosctl preflight
ps aux | grep aos-worker
df -h var/
```

---

## References

[OPERATIONS.md](../OPERATIONS.md) | [TROUBLESHOOTING.md](../TROUBLESHOOTING.md) | [ARCHITECTURE.md](../ARCHITECTURE.md)
