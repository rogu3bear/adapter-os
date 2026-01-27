# Canonical var/ Directory Structure

This document defines the **anchor** for var/ directory contents. Any files/directories not listed here are considered ephemeral and may be deleted.

## Environment Control

```bash
AOS_VAR_DIR=/path/to/var  # Override var/ location (default: ./var)
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
rm -f ./var/*-test.sqlite3* ./var/*_test.sqlite3*

# Clean var/tmp if present
rm -rf ./var/tmp

# Clean old logs (keep last 3 days)
find ./var/logs -name "aos-cp.*" -mtime +3 -delete
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
