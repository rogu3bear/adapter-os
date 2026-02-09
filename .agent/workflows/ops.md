---
description: Operations - start, debug, diagnostics, setup
---

# Operations Workflow

// turbo-all

## Start Services

```bash
./start
```

Web UI: http://localhost:8080

## System Health

```bash
./aosctl check
./aosctl status
./aosctl diag
```

## Debug Logging

```bash
RUST_LOG=debug ./aosctl serve --plan default
```

## Diagnostics Bundle

```bash
mkdir -p var/diag
./aosctl diag bundle --output var/diag/diag_bundle.tar.gz
```

## Free Blocked Ports

```bash
./scripts/free-ports.sh
```

## Graceful Shutdown

```bash
./scripts/graceful-shutdown.sh
```

---

## Initial Setup (new developers)

```bash
cp .env.example .env
cp .githooks/pre-commit-architectural .git/hooks/pre-commit
cargo build --release --workspace
ln -sf target/release/aosctl ./aosctl
./scripts/download-model.sh
./aosctl db migrate
./aosctl check
```
