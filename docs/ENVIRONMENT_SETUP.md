# adapterOS Environment Setup Guide

**Version:** 0.12.0

Comprehensive guide for configuring adapterOS environment variables and settings.

---

## Overview

adapterOS uses environment variables for configuration with the following precedence:

```
CLI arguments > Environment variables > .env file > Built-in defaults
```

This guide covers all available configuration options and their usage.

---

## Quick Setup

### Using the Interactive Setup Script

For first-time setup, use the interactive script:

```bash
./scripts/setup_env.sh
```

This script:
- Creates `.env` from `.env.example`
- Guides you through profile selection (development/training/production)
- Auto-configures environment variables
- Validates your setup

### Manual Setup

1. Copy the example file:
   ```bash
   cp .env.example .env
   ```

2. Edit `.env` with your preferred settings

3. Validate configuration:
   ```bash
   ./scripts/validate_env.sh
   ```

---

## Configuration Sections

### Model Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `AOS_MODEL_PATH` | `/var/models/Llama-3.2-3B-Instruct-4bit` | Path to model directory with config.json and weights |
| `AOS_MANIFEST_HASH` | `756be0c4434c3fe5e1198fcf417c52a662e7a24d0716dbf12aae6246bea84f9e` | Canonical manifest hash (preferred over path) |
| `AOS_MODEL_BACKEND` | `mlx` | Backend preference: `auto`, `coreml`, `metal`, `mlx` |
| `AOS_MODEL_ARCHITECTURE` | *(auto-detected)* | Model architecture (qwen2, llama, etc.) |
| `AOS_WORKER_MANIFEST` | *(auto-resolved)* | Fallback manifest path |
| `AOS_TOKENIZER_PATH` | *(auto-discovered)* | Path to tokenizer.json |

**Backend Options:**
- `auto`: Automatically select best available (CoreML > Metal > MLX)
- `coreml`: Apple Neural Engine (production, guaranteed determinism)
- `metal`: Metal GPU backend (fallback, guaranteed determinism)
- `mlx`: MLX backend (primary, HKDF-seeded determinism)

### Server Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `AOS_SERVER_HOST` | `127.0.0.1` | Server bind address |
| `AOS_SERVER_PORT` | `8080` | Server port (use offsets for multi-dev: 8180, 8280) |
| `AOS_SERVER_WORKERS` | *(CPU cores)* | Number of worker threads |
| `AOS_SERVER_UDS_SOCKET` | `/var/run/aos/aos.sock` | Unix domain socket (production mode) |
| `AOS_SERVER_PRODUCTION_MODE` | `false` | Enable production security requirements |
| `AOS_UI_PORT` | `3200` | UI development server port |
| `AOS_PANEL_PORT` | `3301` | Service panel management port |

### Database Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `AOS_DATABASE_URL` | `sqlite:var/aos-cp.sqlite3` | Database connection URL |
| `AOS_DATABASE_POOL_SIZE` | `10` | Connection pool size |
| `AOS_DATABASE_TIMEOUT` | `30` | Query timeout in seconds |

### Storage Backend Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `AOS_STORAGE_BACKEND` | `sql` | Storage backend: `sql`, `dual`, `kv-primary`, `kv-only` |
| `AOS_KV_PATH` | `var/aos-kv.redb` | KV database file path (redb) |
| `AOS_TANTIVY_PATH` | `var/aos-search` | Tantivy search index directory |

**Storage Backend Migration:**
- `sql`: SQL only (current default)
- `dual`: Write to both SQL and KV, read from SQL (migration phase 1)
- `kv-primary`: Write to both SQL and KV, read from KV (migration phase 2)
- `kv-only`: KV only (future target)

### Security Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `AOS_SECURITY_JWT_MODE` | `eddsa` | JWT mode: `eddsa` (production) or `hs256` (development) |
| `AOS_SECURITY_JWT_SECRET` | *(required)* | JWT secret (generate with: `openssl rand -base64 32`) |
| `AOS_SECURITY_JWT_TTL` | `8h` | JWT token time-to-live |
| `AOS_SECURITY_PF_DENY` | `false` | Enable PF deny rules (required in production) |

### Logging Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `AOS_LOG_LEVEL` | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `AOS_LOG_FORMAT` | `text` | Log format: `text`, `json` |
| `AOS_LOG_FILE` | *(stderr)* | Optional log file path |

**Log Level Examples:**
```
info                                    # Global info level
info,adapteros=debug                   # Global info, adapteros debug
info,adapteros_lora_worker=trace       # Worker tracing
```

### Memory Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `AOS_MEMORY_HEADROOM_PCT` | `0.15` | Memory headroom percentage (0.0-1.0) |

---

## Profile Configurations

### Development Profile

Recommended for local development and testing:

```bash
# Model Configuration
AOS_MODEL_BACKEND=mlx
AOS_MLX_PRECISION=float16
AOS_MLX_MEMORY_POOL_ENABLED=true
AOS_MLX_MAX_MEMORY=0

# Server Configuration
AOS_SERVER_PRODUCTION_MODE=false
AOS_SECURITY_JWT_MODE=hs256

# Database
AOS_DATABASE_URL=sqlite:var/aos-cp.sqlite3

# Logging
AOS_LOG_LEVEL=debug
```

### Training Profile

For model training workflows:

```bash
# Model Configuration
AOS_MODEL_BACKEND=mlx
AOS_MLX_PRECISION=float32  # Higher precision for training

# Server Configuration
AOS_SERVER_PRODUCTION_MODE=false
AOS_SECURITY_JWT_MODE=hs256

# Database
AOS_DATABASE_URL=sqlite:var/aos-training.sqlite3

# Memory (higher headroom for training)
AOS_MEMORY_HEADROOM_PCT=0.25

# Logging
AOS_LOG_LEVEL=info
```

### Production Profile

For production deployments:

```bash
# Model Configuration
AOS_MODEL_BACKEND=coreml  # Deterministic production backend

# Server Configuration
AOS_SERVER_PRODUCTION_MODE=true
AOS_SECURITY_JWT_MODE=eddsa
AOS_SECURITY_JWT_SECRET=<your-secret>
AOS_SECURITY_PF_DENY=true
AOS_SERVER_UDS_SOCKET=/var/run/aos/aos.sock

# Database
AOS_DATABASE_URL=sqlite:/var/lib/aos/aos.sqlite3

# Logging
AOS_LOG_LEVEL=warn
AOS_LOG_FILE=/var/log/aos/aos.log
```

---

## Environment-Specific Setup

### Multi-Developer Setup

For teams with multiple developers, use port offsets to avoid conflicts:

```bash
# Developer A (ports 8080, 3200, 3301)
AOS_SERVER_PORT=8080
AOS_UI_PORT=3200
AOS_PANEL_PORT=3301

# Developer B (ports 8180, 3300, 3401)
AOS_SERVER_PORT=8180
AOS_UI_PORT=3300
AOS_PANEL_PORT=3401

# Developer C (ports 8280, 3400, 3501)
AOS_SERVER_PORT=8280
AOS_UI_PORT=3400
AOS_PANEL_PORT=3501
```

### Docker Setup

For containerized deployments:

```bash
# Use host networking or explicit ports
AOS_SERVER_HOST=0.0.0.0
AOS_SERVER_PORT=8080

# Volume mount paths
AOS_MODEL_PATH=/app/models
AOS_DATABASE_URL=sqlite:/app/data/aos.sqlite3
AOS_KV_PATH=/app/data/aos-kv.redb
AOS_TANTIVY_PATH=/app/data/aos-search

# Production security
AOS_SERVER_PRODUCTION_MODE=true
AOS_SECURITY_JWT_MODE=eddsa
AOS_SECURITY_JWT_SECRET=<container-secret>
```

---

## Validation and Troubleshooting

### Configuration Validation

Use the validation script to check your setup:

```bash
./scripts/validate_env.sh
```

This checks:
- Required environment variables
- File/directory permissions
- Database connectivity
- Model configuration validity

### Common Issues

#### JWT Secret Missing

**Error:** `JWT secret not configured`
**Fix:** Generate and set `AOS_SECURITY_JWT_SECRET`:
```bash
openssl rand -base64 32
```

#### Model Path Not Found

**Error:** `Model directory not found`
**Fix:** Ensure `AOS_MODEL_PATH` points to a valid model directory with `config.json`

#### Port Conflicts

**Error:** `Port already in use`
**Fix:** Use port offsets for multi-developer setups (see above)

#### Database Permission Issues

**Error:** `Database connection failed`
**Fix:** Ensure the database directory is writable:
```bash
mkdir -p var
chmod 755 var
```

---

## Advanced Configuration

### Runtime Variable Overrides

Override `.env` settings at runtime:

```bash
# Override specific variables
AOS_LOG_LEVEL=trace ./start up

# Multiple overrides
AOS_LOG_LEVEL=debug AOS_MODEL_BACKEND=metal ./start up
```

### Configuration Profiles

Switch between predefined profiles:

```bash
# Switch to production profile
./scripts/switch_env_profile.sh production

# Switch to development profile
./scripts/switch_env_profile.sh development
```

### Environment File Inclusion

Include additional environment files:

```bash
# Load base config, then override
cp .env.example .env
echo "AOS_LOG_LEVEL=debug" >> .env
echo "AOS_MODEL_BACKEND=mlx" >> .env
```

---

## Related Documentation

- [**CONFIGURATION.md**](CONFIGURATION.md) — Configuration system overview
- [**DEPLOYMENT.md**](DEPLOYMENT.md) — Production deployment guide
- [**QUICKSTART.md**](../QUICKSTART.md) — Getting started guide
- [**SECURITY.md**](SECURITY.md) — Security configuration guide

---

*Last updated: January 13, 2026*