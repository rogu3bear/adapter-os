# OPERATIONS

---

## Health

```bash
curl -f http://localhost:18080/healthz
curl -f http://localhost:18080/readyz
./aosctl doctor
./aosctl preflight
```

**Handlers:** `handlers::health`, `handlers::ready`. Readiness checks DB, worker, models (configurable timeouts).

---

## Logs

`var/logs/` (config: `logging.log_dir`). Rotated per `logging.rotation`.

---

## Config

`configs/cp.toml`. See [CONFIGURATION.md](CONFIGURATION.md).

---

## Runbooks

[runbooks/](runbooks/) for incident procedures.
