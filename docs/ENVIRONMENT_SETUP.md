# AdapterOS Environment Configuration Guide

**Purpose:** Quick reference for setting up environment variables for AdapterOS development and deployment.

**Source:** `.env.example` | **Last Updated:** 2025-11-23

---

## Quick Start

```bash
# Step 1: Copy environment template
cp .env.example .env

# Step 2: Edit for your setup
vim .env

# Step 3: Verify configuration
cargo run -p adapteros-orchestrator -- config show
```

---

## Configuration Profiles

Choose the profile matching your use case:

### Development (Recommended for Local Testing)

**Characteristics:** All features enabled, debug logging, insecure defaults for convenience.

**.env setup:**
```bash
RUST_LOG=debug,adapteros=trace
AOS_SERVER_PRODUCTION_MODE=false
AOS_SECURITY_JWT_MODE=hs256
AOS_DATABASE_URL=sqlite:var/aos-cp.sqlite3
AOS_MODEL_PATH=./var/models/Qwen2.5-7B-Instruct-4bit
AOS_WORKER_MANIFEST=./manifests/qwen32b-coder-mlx.yaml
AOS_MANIFEST_HASH=756be0c4434c3fe5e1198fcf417c52a662e7a24d0716dbf12aae6246bea84f9e
AOS_MODEL_BACKEND=mlx
```

**Use when:**
- Running locally on your machine
- Debugging issues
- Testing new features
- Experimenting with different model backends

**Ports:**
- Backend API: `8080`
- UI development server: `3200` (shared for dev/prod tooling)
- Service panel: `3301`

---

### Training (ML Model Fine-Tuning)

**Characteristics:** MLX backend with GPU acceleration, float16 precision, memory pool enabled.

**.env setup:**
```bash
AOS_MODEL_BACKEND=mlx
AOS_MODEL_PATH=./var/models/Qwen2.5-7B-Instruct-4bit
AOS_WORKER_MANIFEST=./manifests/qwen32b-coder-mlx.yaml
AOS_MANIFEST_HASH=756be0c4434c3fe5e1198fcf417c52a662e7a24d0716dbf12aae6246bea84f9e
AOS_MLX_PRECISION=float16
AOS_MLX_MEMORY_POOL_ENABLED=true
AOS_MLX_MAX_MEMORY=0
RUST_LOG=info,adapteros_lora_mlx_ffi=debug
AOS_DATABASE_URL=sqlite:var/aos-cp.sqlite3
```

**Use when:**
- Training new LoRA adapters
- Running experiments
- Using MLX for production inference or training
- Need GPU-accelerated computation

**Requirements:**
- MLX library installed: `pip install mlx` (optional for testing)
- Model in MLX format with `config.json`, `tokenizer.json`, weights

---

### Production (Secure Serving)

**Characteristics:** CoreML backend for ANE acceleration, maximum security, Ed25519 JWT, audit logging.

**.env setup:**
```bash
AOS_SERVER_PRODUCTION_MODE=true
AOS_MODEL_BACKEND=coreml
AOS_SECURITY_JWT_MODE=eddsa
AOS_SECURITY_JWT_SECRET=<generate-with-openssl>
AOS_SECURITY_PF_DENY=true
AOS_SERVER_UDS_SOCKET=/var/run/aos/aos.sock
AOS_DATABASE_URL=sqlite:/var/lib/aos/cp.db
RUST_LOG=warn,adapteros=info
AOS_TELEMETRY_ENABLED=true
```

**Use when:**
- Serving inference at scale
- Enforcing security policies
- Auditing required (compliance, regulations)
- Production-grade reliability needed

**Requirements:**
- macOS 13+ with Apple Silicon (M1+)
- Xcode Command Line Tools
- CoreML model or conversion pipeline
- JWT secret (generate: `openssl rand -base64 32`)
- UDS socket directory: `sudo mkdir -p /var/run/aos && sudo chmod 755 /var/run/aos`

---

## Variable Reference

### Model Configuration

| Variable | Default | Purpose | Example |
|----------|---------|---------|---------|
| `AOS_MODEL_PATH` | `./var/models/Qwen2.5-7B-Instruct-4bit` | Base model directory | `./var/models/Qwen2.5-7B-Instruct-4bit` |
| `AOS_MANIFEST_HASH` | `756be0c4434c3fe5e1198fcf417c52a662e7a24d0716dbf12aae6246bea84f9e` | Manifest hash (preferred contract) | Same as default |
| `AOS_MODEL_BACKEND` | `mlx` | Backend selection | `mlx`, `coreml`, `metal`, `auto` |
| `AOS_MODEL_ARCHITECTURE` | Auto-detect | Model type (Qwen2, Llama, etc.) | `qwen2`, `llama2` |

**Notes:**
- `AOS_MODEL_PATH` must contain `config.json` and model weights
- `AOS_MANIFEST_HASH` is the routing contract; workers fetch/verify by hash
- `AOS_MODEL_BACKEND=mlx` by default; `auto` selects CoreML > Metal > MLX
- Model auto-detected from `config.json` if architecture not specified

---

### Server Configuration

| Variable | Default | Purpose | Example |
|----------|---------|---------|---------|
| `AOS_SERVER_HOST` | `127.0.0.1` | Bind address | `0.0.0.0` (bind all), `192.168.1.100` (specific) |
| `AOS_SERVER_PORT` | `8080` | API server port | `8000`, `9000` |
| `AOS_SERVER_WORKERS` | CPU cores | Worker thread count | `4`, `8` |
| `AOS_SERVER_PRODUCTION_MODE` | `false` | Enable production constraints | `true` (production), `false` (dev) |
| `AOS_SERVER_UDS_SOCKET` | unset | Unix domain socket (production only) | `/var/run/aos/aos.sock` |
| `AOS_UI_PORT` | `3200` | Vite dev server port | `3000`, `5173` |
| `AOS_PANEL_PORT` | `3301` | Service panel port | `3300` |

**Production Requirements:**
- When `AOS_SERVER_PRODUCTION_MODE=true`:
  - `AOS_SERVER_UDS_SOCKET` must be set
  - `AOS_SECURITY_JWT_MODE` must be `eddsa`
  - `AOS_SECURITY_PF_DENY` must be `true`

---

### Database Configuration

| Variable | Default | Purpose | Example |
|----------|---------|---------|---------|
| `AOS_DATABASE_URL` | `sqlite:var/aos-cp.sqlite3` | Database connection string | `sqlite:/path/to/db.sqlite3` |
| `AOS_DATABASE_POOL_SIZE` | `10` | Connection pool size | `5`, `20` |
| `AOS_DATABASE_TIMEOUT` | `30` | Query timeout (seconds) | `60`, `120` |

**Connection String Formats:**
- SQLite (development): `sqlite:var/aos-cp.sqlite3`
- SQLite (absolute path): `sqlite:/var/lib/aos/cp.db`

**Notes:**
- SQLite always uses WAL mode for safety
- Pool size should match expected concurrent connections
- Timeout covers all database operations

---

### Security Configuration

| Variable | Default | Purpose | Example |
|----------|---------|---------|---------|
| `AOS_SECURITY_JWT_MODE` | `eddsa` | JWT signing algorithm | `eddsa` (Ed25519, production), `hs256` (HMAC, dev-only) |
| `AOS_SECURITY_JWT_SECRET` | unset | JWT signing secret | Generate: `openssl rand -base64 32` |
| `AOS_SECURITY_JWT_TTL` | `8h` | Token time-to-live | `1h`, `24h` |
| `AOS_SECURITY_PF_DENY` | `false` | Enable PF deny rules | `true` (production), `false` (dev) |

**JWT Generation:**
```bash
# Generate a new JWT secret
openssl rand -base64 32

# Example output:
# 1a2b3c4d5e6f7g8h9i0j1k2l3m4n5o6p7q8r9s0=
```

**Ed25519 (Production):**
- Uses 256-bit Ed25519 keys
- Guaranteed authenticity and non-repudiation
- Required for `AOS_SERVER_PRODUCTION_MODE=true`

**HS256 (Development-Only):**
- Simple HMAC-SHA256
- Requires `AOS_SECURITY_JWT_SECRET`
- Must NOT be used in production

---

### Logging Configuration

| Variable | Default | Purpose | Example |
|----------|---------|---------|---------|
| `RUST_LOG` | `info,adapteros=debug` | Log level specification | `debug`, `trace`, `info,myapp=debug` |
| `AOS_LOG_FORMAT` | `text` | Output format | `text` (human-readable), `json` (structured) |
| `AOS_LOG_FILE` | stderr | Log file path | `/var/log/aos/aos.log` |

**Log Levels (highest to lowest):**
```
ERROR   - Critical failures (action required)
WARN    - Warnings (attention needed)
INFO    - General information (normal operations)
DEBUG   - Development details (diagnostics)
TRACE   - Detailed debug (very verbose)
```

**Module-Specific Configuration:**
```bash
# Backend debug info, everything else at info
RUST_LOG=info,adapteros_lora_mlx_ffi=debug

# Trace router decisions, debug adapters, warn everything else
RUST_LOG=warn,adapteros_lora_router=trace,adapteros_lora_worker=debug

# Development (very verbose)
RUST_LOG=debug,adapteros=trace
```

**JSON Format Example:**
```bash
AOS_LOG_FORMAT=json
# Output: {"timestamp":"2025-11-23T...","level":"INFO","module":"adapteros","message":"..."}
```

---

### Memory Configuration

| Variable | Default | Purpose | Example |
|----------|---------|---------|---------|
| `AOS_MEMORY_HEADROOM_PCT` | `0.15` | Headroom to maintain | `0.2` (20%), `0.1` (10%) |
| `AOS_MEMORY_EVICTION_THRESHOLD` | `0.85` | Trigger eviction at (%) | `0.9` (90%), `0.75` (75%) |

**Memory Management:**
- **Headroom:** Percentage of RAM to keep free for system operations
  - `0.15` = keep 15% free (evict when usage > 85%)
  - Higher = more aggressive eviction, lower latency
  - Lower = more adapter capacity, risk of system slowdown

- **Eviction threshold:** When to start evicting adapters
  - `0.85` = evict when system usage exceeds 85% of total RAM
  - Adapters evicted by LRU (least recently used)

**Example (16GB Mac):**
```bash
# Keep 2.4GB free (15% of 16GB)
AOS_MEMORY_HEADROOM_PCT=0.15

# Evict when total usage exceeds 13.6GB (85% of 16GB)
AOS_MEMORY_EVICTION_THRESHOLD=0.85
```

---

### Backend Configuration

| Variable | Default | Purpose | Example |
|----------|---------|---------|---------|
| `AOS_BACKEND_COREML_ENABLED` | `true` | Enable CoreML backend | `false` to disable |
| `AOS_BACKEND_METAL_ENABLED` | `true` | Enable Metal backend | `false` to disable |
| `AOS_BACKEND_MLX_ENABLED` | `true` | Enable MLX backend | `false` to disable |
| `AOS_BACKEND_GPU_INDEX` | `0` | GPU device index (multi-GPU) | `1`, `2` |

---

### MLX Backend Configuration

| Variable | Default | Purpose | Example |
|----------|---------|---------|---------|
| `AOS_MLX_FFI_MODEL` | unset | MLX model directory | `./var/models/Qwen2.5-7B-Instruct-4bit` |
| `AOS_MLX_MAX_MEMORY` | `0` (unlimited) | Max memory (bytes) | `16000000000` (16GB) |
| `AOS_MLX_MEMORY_POOL_ENABLED` | `true` | Enable memory pool | `false` to disable |
| _Quantization_ | model-defined | Quantization/precision is fixed per backend | model weights (int4/int8/fp16) |

**Quantization Note:** Quantization/precision is fixed per backend. MLX uses the model’s packaged weights (int4/int8/fp16) without per-request or per-token overrides. Metal/CoreML run backend-fixed fp16/bf16 kernels.

---

### Telemetry Configuration

| Variable | Default | Purpose | Example |
|----------|---------|---------|---------|
| `AOS_TELEMETRY_ENABLED` | `false` | Enable telemetry collection | `true` (production) |
| `AOS_TELEMETRY_EXPORT_INTERVAL` | `60s` | Export frequency | `30s`, `5m` |

**Telemetry Data Collected:**
- Router decisions (adapter selections)
- Inference latencies and throughput
- Memory usage patterns
- Error rates and recovery events
- Policy violations (if enabled)

---

### Federation Configuration

| Variable | Default | Purpose | Example |
|----------|---------|---------|---------|
| `AOS_FEDERATION_ENABLED` | `false` | Enable federation | `true` for multi-node |
| `AOS_FEDERATION_NODE_ID` | auto-generated | Node identifier | `prod-node-001` |
| `AOS_FEDERATION_PEERS` | empty | Comma-separated peer addresses | `peer1.com:9000,peer2.com:9000` |

---

### Debug Flags (Development Only)

**DANGER: Do not use in production!**

| Variable | Default | Purpose |
|----------|---------|---------|
| `AOS_DEBUG_DETERMINISTIC` | `false` | Enable determinism debug logging |
| `AOS_DEBUG_SKIP_KERNEL_SIG` | `false` | Skip kernel signature verification (DEV-ONLY; blocked when `AOS_SERVER_PRODUCTION_MODE` is set) |

---

## Configuration Precedence

Variables are loaded in this order (highest to lowest priority):

1. **CLI Arguments** - `aosctl serve --port 9000`
2. **Environment Variables** - `export AOS_SERVER_PORT=9000`
3. **.env File** - `AOS_SERVER_PORT=9000` in `.env`
4. **Defaults** - Built into code

**Example:**
```bash
# .env contains
AOS_SERVER_PORT=8080

# CLI overrides .env
aosctl serve --port 9000  # Uses 9000, not 8080

# Environment overrides .env
export AOS_SERVER_PORT=8081
aosctl serve              # Uses 8081, not 8080
```

---

## Environment Setup Checklist

### New Developer Setup

- [ ] Clone repository: `git clone https://github.com/rogu3bear/adapter-os.git`
- [ ] Copy environment: `cp .env.example .env`
- [ ] Download model: `./scripts/download_model.sh`
- [ ] Initialize database: `cargo run -p adapteros-orchestrator -- db migrate`
- [ ] Create default tenant: `cargo run -p adapteros-orchestrator -- init-tenant --id default --uid 1000 --gid 1000`
- [ ] Verify configuration: `cargo run -p adapteros-orchestrator -- config show`
- [ ] Start backend: `cargo run --release -p adapteros-server-api`
- [ ] Start UI: `cd ui && pnpm install && pnpm dev`
- [ ] Access UI: Open http://localhost:3200

### Production Deployment

- [ ] Set `AOS_SERVER_PRODUCTION_MODE=true`
- [ ] Generate JWT secret: `openssl rand -base64 32`
- [ ] Set `AOS_SECURITY_JWT_SECRET` to generated value
- [ ] Set `AOS_SECURITY_JWT_MODE=eddsa`
- [ ] Set `AOS_SECURITY_PF_DENY=true`
- [ ] Create UDS socket directory: `mkdir -p /var/run/aos`
- [ ] Set `AOS_SERVER_UDS_SOCKET=/var/run/aos/aos.sock`
- [ ] Update `AOS_DATABASE_URL` to production path
- [ ] Set `AOS_TELEMETRY_ENABLED=true`
- [ ] Set `RUST_LOG=warn,adapteros=info`
- [ ] Test configuration: `cargo run -p adapteros-orchestrator -- config show`
- [ ] Build release binary: `cargo build --release -p adapteros-server-api`
- [ ] Deploy and start service

---

## Troubleshooting

### Variable Not Taking Effect

**Problem:** Changed `.env` but server still uses old value.

**Solution:** Environment variables must be loaded before starting the process.
```bash
# Option 1: Export from .env
export $(grep -v '^#' .env | xargs)

# Option 2: Use cargo run (automatically loads .env)
cargo run --release -p adapteros-server-api

# Option 3: Use .env.local in development
cp .env .env.local
# .env.local is loaded by most Rust frameworks
```

### Port Already in Use

**Problem:** `Error: bind: Address already in use`

**Solution:**
```bash
# Find process using port
lsof -i :8080

# Kill process
pkill -f adapteros-server

# Or use different port
AOS_SERVER_PORT=9000 cargo run --release -p adapteros-server-api
```

### Database Connection Failed

**Problem:** `Error: unable to open database: no such file or directory`

**Solution:**
```bash
# Verify AOS_DATABASE_URL
echo $AOS_DATABASE_URL

# Create directory if needed
mkdir -p var

# Run migrations
cargo run -p adapteros-orchestrator -- db migrate
```

### Model Not Found

**Problem:** `Error: model not found at ./var/models/Qwen2.5-7B-Instruct-4bit`

**Solution:**
```bash
# Verify AOS_MODEL_PATH or AOS_MLX_FFI_MODEL
echo $AOS_MODEL_PATH

# Download model
./scripts/download_model.sh

# Check model directory
ls -la var/models/Qwen2.5-7B-Instruct-4bit/
# Should contain: config.json, tokenizer.json, model weights
```

### MLX Backend Not Available

**Problem:** `Error: MLX backend not available`

**Solution:**
```bash
# Install MLX library (optional, not required for all uses)
pip install mlx

# Or use different backend
export AOS_MODEL_BACKEND=metal

# Verify available backends
cargo run -p adapteros-orchestrator -- config show | grep backend
```

---

## See Also

- [Configuration System Design](docs/PRD-CONFIG-001-unified-configuration-system.md) - Architecture and rationale
- [Local Build Guide](docs/LOCAL_BUILD.md) - Build troubleshooting
- [Quick Start Guide](QUICKSTART.md) - 10-minute setup
- [MLX Integration Guide](docs/MLX_INTEGRATION.md) - MLX backend details
- [CoreML Integration Guide](docs/COREML_INTEGRATION.md) - CoreML/ANE details

---

**Last Updated:** 2025-11-23 | **Maintainer:** James KC Auchterlonie
MLNavigator Inc 2025-12-07.
