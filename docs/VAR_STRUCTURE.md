# Canonical var/ Directory Structure

This document defines the **anchor** for var/ directory contents. Any files/directories not listed here are considered ephemeral and may be deleted.

> **Path Format**: The canonical form is `var/` (NOT `./var/`). All code and config must use `var/...` without the leading `./`. This is enforced project-wide.

## Environment Control

```bash
AOS_VAR_DIR=/path/to/var  # Override var/ location (default: var)
```

## Canonical Structure

```
var/
├── aos-cp.sqlite3          # Control plane database (REQUIRED)
├── aos-cp.sqlite3-shm      # SQLite WAL shared memory
├── aos-cp.sqlite3-wal      # SQLite write-ahead log
├── adapters/               # Trained LoRA adapters (persistent)
├── models/                 # Base model files (large, persistent)
├── model-cache/            # Cached model downloads
├── keys/                   # Signing keys (sensitive, persistent)
├── logs/                   # Application logs (rotated)
├── run/                    # Runtime sockets and status files
├── telemetry/              # Telemetry event logs
├── manifest-cache/         # Cached manifest files
├── embeddings/             # Embedding model data
├── documents/              # Ingested documents
├── datasets/               # Training datasets
├── quarantine/             # Quarantined items pending review
├── analysis/               # Analysis outputs
├── audit-evidence/         # Audit trail artifacts
├── demo/                   # Demo data and samples
├── active_learning/        # Active learning queue
└── bundles/                # Packaged adapter bundles
```

## Logging Surfaces (Dev)

- `var/logs/*`: runtime/process logs from `./start` and `scripts/service-manager.sh`
- `var/aos-cp.sqlite3` table `client_errors`: structured UI/client errors (including UI panics)
- `/errors` page: reads from `client_errors` (not from flat files in `var/logs`)

## Forbidden Paths

The system **rejects** these paths for persistent storage (enforced in `path_security.rs`):
- `/tmp`
- `/private/tmp`
- `/var/tmp`

## Ephemeral (NOT Canonical)

These patterns indicate test/development artifacts that should be cleaned:
- `*-test.sqlite3`, `*_test.sqlite3` - Test databases
- `*.tmp`, `*.tmp.*` - Temporary files
- `tmp/` subdirectories - Test isolation directories
- Crate-level `var/` directories (e.g., `crates/*/var/`)
- UUID-named directories - Integration test artifacts

## Cleanup Commands

```bash
# Clean crate-level var directories
find ./crates -type d -name "var" -not -path "*/target/*" -exec rm -rf {} +

# Clean test databases
rm -f var/*-test.sqlite3* var/*_test.sqlite3*

# Clean var/tmp if present
rm -rf var/tmp

# Clean old runtime logs (keep last 7 days)
find var/logs -maxdepth 1 -type f -mtime +7 -delete

# Clean old UI/client error rows (keep last 30 days)
sqlite3 var/aos-cp.sqlite3 \
  "DELETE FROM client_errors WHERE created_at < datetime('now', '-30 days');"
```

## Size Budget

| Directory | Expected Size | Notes |
|-----------|--------------|-------|
| models/ | 10-20 GB | Base models (Qwen, Mistral, etc.) |
| model-cache/ | 0-10 GB | Downloaded model cache |
| adapters/ | 10-100 MB | Trained LoRA weights |
| logs/ | <100 MB | Rotated logs |
| aos-cp.sqlite3 | <50 MB | Control plane database |
| Everything else | <1 GB | Runtime state |

**Total budget**: ~30 GB maximum for development

## Validation

Run this to verify var/ matches canonical structure:
```bash
./aosctl doctor  # Includes var/ structure validation
```
