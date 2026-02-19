# TROUBLESHOOTING

---

## Blank UI

```bash
./scripts/build-ui.sh
```

UI assets must exist in `crates/adapteros-server/static/`. Backend serves from there.

---

## Port in use

```bash
./scripts/fresh-build.sh
```

Stops services, frees ports.

---

## Health fails

```bash
./aosctl doctor
./aosctl preflight
```

Check `var/logs/` for errors. Readiness checks: DB, worker, models. See `handlers::ready`, `ReadyzQuery`.

---

## Migration issues

```bash
./aosctl db migrate
# Verify: migrations/signatures.json
```

---

## Runbooks

[runbooks/](runbooks/) for incident procedures.
