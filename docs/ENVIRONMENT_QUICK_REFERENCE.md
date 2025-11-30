# AdapterOS Environment Quick Reference

**TL;DR - Get running in 3 commands:**

```bash
cp .env.example .env              # Create environment file
./scripts/setup_env.sh             # Interactive setup (recommended)
cargo run --release -p adapteros-server-api  # Start server
```

---

## Quick Setup

### Option A: Interactive Setup (Recommended)

```bash
./scripts/setup_env.sh
```

This script will:
1. Copy `.env.example` → `.env`
2. Ask about your use case (dev, training, or production)
3. Configure environment automatically
4. Setup database and download model
5. Validate configuration

### Option B: Manual Setup

```bash
# Step 1: Copy template
cp .env.example .env

# Step 2: Edit for your setup
vim .env

# Step 3: Verify
./scripts/validate_env.sh

# Step 4: Initialize (if first time)
cargo run -p adapteros-orchestrator -- db migrate
```

---

## Environment Profiles

### Development (Local Testing)

**Perfect for:** Debugging, feature development, experimentation

```bash
RUST_LOG=debug,adapteros=trace
AOS_SERVER_PRODUCTION_MODE=false
AOS_SECURITY_JWT_MODE=hs256
AOS_MODEL_BACKEND=auto
```

**Start:**
```bash
cargo run --release -p adapteros-server-api
```

### Training (Model Fine-Tuning)

**Perfect for:** Training LoRA adapters, GPU-accelerated ML workloads

```bash
AOS_MODEL_BACKEND=mlx
AOS_MLX_PRECISION=float16
AOS_MLX_MEMORY_POOL_ENABLED=true
RUST_LOG=info,adapteros_lora_mlx_ffi=debug
```

**Start:**
```bash
cargo run --release -p adapteros-server-api
```

### Production (Secure Serving)

**Perfect for:** Production inference, compliance requirements, auditing

```bash
AOS_SERVER_PRODUCTION_MODE=true
AOS_MODEL_BACKEND=coreml
AOS_SECURITY_JWT_MODE=eddsa
AOS_SECURITY_JWT_SECRET=$(openssl rand -base64 32)
AOS_SECURITY_PF_DENY=true
AOS_TELEMETRY_ENABLED=true
```

**Setup:**
```bash
# Create UDS socket directory
sudo mkdir -p /var/run/aos && sudo chmod 755 /var/run/aos

# Build and deploy
cargo build --release -p adapteros-server-api
```

---

## Essential Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `AOS_MODEL_PATH` | `./models/qwen2.5-7b-mlx` | Where model files are stored |
| `AOS_MODEL_BACKEND` | `auto` / `mlx` / `coreml` / `metal` | Which backend to use |
| `AOS_SERVER_PORT` | `8080` | API server port |
| `AOS_DATABASE_URL` | `sqlite:var/aos-cp.sqlite3` | Database connection |
| `RUST_LOG` | `debug,adapteros=trace` | Log level |

---

## File Structure

```
.env.example          ← Template (tracked in git)
.env                  ← Your config (git-ignored, don't commit)
docs/ENVIRONMENT_SETUP.md  ← Full reference
scripts/setup_env.sh       ← Interactive setup helper
scripts/validate_env.sh    ← Verify configuration
```

---

## Common Tasks

### Download a Model

```bash
./scripts/download_model.sh
# Downloads Qwen 2.5 7B MLX format (~3.8GB)
```

### Initialize Database

```bash
cargo run -p adapteros-orchestrator -- db migrate
cargo run -p adapteros-orchestrator -- init-tenant --id default --uid 1000 --gid 1000
```

### Check Configuration

```bash
./scripts/validate_env.sh
# Shows what's configured and any issues
```

### Change Backend

```bash
# Edit .env
export AOS_MODEL_BACKEND=mlx    # Development/Training
export AOS_MODEL_BACKEND=coreml # Production
export AOS_MODEL_BACKEND=metal  # Fallback

cargo run --release -p adapteros-server-api
```

### View Log Output

```bash
# Edit .env
RUST_LOG=debug                    # General debug
RUST_LOG=trace,adapteros=debug    # Very verbose
RUST_LOG=info,adapteros=debug     # Less verbose
```

---

## Troubleshooting

### Port Already in Use

```bash
# Find what's using port 8080
lsof -i :8080

# Use different port
AOS_SERVER_PORT=9000 cargo run --release -p adapteros-server-api
```

### Model Not Found

```bash
# Check AOS_MODEL_PATH
echo $AOS_MODEL_PATH

# Download model
./scripts/download_model.sh
```

### Database Errors

```bash
# Reset database (development only!)
rm var/aos-cp.sqlite3*
cargo run -p adapteros-orchestrator -- db migrate
```

### Variable Not Taking Effect

```bash
# Export variables first
export $(grep -v '^#' .env | xargs)

# Or use this pattern
cargo run --release -p adapteros-server-api  # Auto-loads .env
```

---

## Production Checklist

- [ ] `AOS_SERVER_PRODUCTION_MODE=true`
- [ ] `AOS_SECURITY_JWT_MODE=eddsa`
- [ ] `AOS_SECURITY_JWT_SECRET` set (use `openssl rand -base64 32`)
- [ ] `AOS_SECURITY_PF_DENY=true`
- [ ] `AOS_SERVER_UDS_SOCKET` set to production path
- [ ] `AOS_TELEMETRY_ENABLED=true`
- [ ] `RUST_LOG=warn,adapteros=info`
- [ ] `.env` file has mode `600` (chmod 600 .env)
- [ ] Database path is on durable storage
- [ ] UDS socket directory is writable
- [ ] Configuration validated: `./scripts/validate_env.sh`

---

## See Also

- **Full Reference:** [docs/ENVIRONMENT_SETUP.md](docs/ENVIRONMENT_SETUP.md)
- **Quick Start:** [QUICKSTART.md](QUICKSTART.md)
- **Architecture:** [docs/ARCHITECTURE_INDEX.md](docs/ARCHITECTURE_INDEX.md)

---

**Save this file for quick reference during development!**
