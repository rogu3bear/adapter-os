# DEPLOYMENT

---

## Start

```bash
./start
```

Backend + worker. Port 8080. UI served from `crates/adapteros-server/static/`.

**Script:** `./start` sources `scripts/lib/env-loader.sh`, `scripts/lib/freeze-guard.sh`. Runs `adapteros-server` and `aos-worker` (or via service manager).

---

## Options

```bash
./start backend    # Backend only
./start worker     # Worker only
./start down       # Graceful shutdown
./start status     # Status
./start preflight  # Checks only
```

---

## Dev

```bash
AOS_DEV_NO_AUTH=1 ./start
```

Bypasses auth. See [SECURITY.md](SECURITY.md).

---

## UI

Backend serves `crates/adapteros-server/static/`. For dev: `cd crates/adapteros-ui && trunk serve` (port 3200).

---

## Build UI

```bash
./scripts/build-ui.sh
```

Output: `crates/adapteros-server/static/`.
