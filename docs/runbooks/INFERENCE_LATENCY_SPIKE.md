# Inference Latency Spike

P99 latency elevated. SEV-2.

---

## Symptoms

- P99 > 300ms
- Slow chat responses
- Timeouts

---

## Diagnosis

```bash
# Check stub backend (CRITICAL - 10-100x slower)
grep -i "stub\|fallback" var/logs/*.log | tail -5

# Metrics
curl -s http://localhost:18080/v1/metrics | jq .

# Memory pressure
./aosctl metrics show 2>/dev/null | grep -E "memory|pressure"
```

---

## Resolution

1. **Stub backend:** Rebuild with `--features mlx-backend`
2. **Memory pressure:** Evict adapters; see [MEMORY_PRESSURE](MEMORY_PRESSURE.md)
3. **Router:** Reduce k_sparse in config
4. **WAL:** `PRAGMA wal_checkpoint(TRUNCATE)`
