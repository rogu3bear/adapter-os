# Memory Pressure

High memory, eviction failures. SEV-2.

---

## Symptoms

- Memory > 85%
- Adapter load failures
- OOM warnings

**Code:** `UmaPressureMonitor` in `adapteros-lora-worker`, `PressureManager` in `adapteros-memory`. Headroom policy: `min_headroom_pct` (config).

---

## Diagnosis

```bash
# macOS
vm_stat

# Loaded adapters
sqlite3 var/aos-cp.sqlite3 "SELECT COUNT(*), SUM(vram_mb) FROM adapters WHERE status='Loaded';"
```

---

## Resolution

```bash
# Force eviction (if API available)
curl -X POST http://localhost:8080/v1/lifecycle/evict \
  -H "Content-Type: application/json" \
  -d '{"strategy":"lowest_activation_pct","count":5}'

# Or restart worker
pkill -f aos-worker
```

**Code:** `LifecycleManager::evict()`, `PressureManager::select_eviction_candidates()`. Pinned adapters (in `pinned_adapters` table) are excluded.

---

## Config

`configs/cp.toml`:

```toml
[memory]
eviction_threshold_pct = 75
min_headroom_pct = 20
```

See [AGENTS.md](../../AGENTS.md#uma-backpressure--eviction).
