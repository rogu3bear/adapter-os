# AdapterOS Environment Configuration Index

Complete reference for environment setup and configuration resources.

---

## Quick Navigation

**First Time Setting Up?** → Start here: [ENVIRONMENT_SETUP.md](ENVIRONMENT_SETUP.md)

**Want Quick Commands?** → Use this: [../ENVIRONMENT_QUICK_REFERENCE.md](../ENVIRONMENT_QUICK_REFERENCE.md)

**Need Helper Scripts?** → See: [../scripts/setup_env.sh](../scripts/setup_env.sh)

---

## Documentation Files

### Setup & Configuration

| Resource | Purpose | When to Use |
|----------|---------|------------|
| [ENVIRONMENT_SETUP.md](ENVIRONMENT_SETUP.md) | Complete environment configuration guide with variable reference | First-time setup, detailed configuration |
| [../ENVIRONMENT_QUICK_REFERENCE.md](../ENVIRONMENT_QUICK_REFERENCE.md) | One-page quick reference card | Daily reference, quick lookups |
| [../QUICKSTART.md](../QUICKSTART.md) | 10-minute quick start guide | New developers, fast setup |
| [PRD-CONFIG-001-unified-configuration-system.md](PRD-CONFIG-001-unified-configuration-system.md) | Configuration system architecture and design | Understanding how config works |
| [../LOCAL_BUILD.md](../LOCAL_BUILD.md) | Local build guide with troubleshooting | Build issues, environment problems |

### Template & Examples

| Resource | Purpose |
|----------|---------|
| [../.env.example](../.env.example) | Template configuration (tracked in git) |
| Configuration profiles in [ENVIRONMENT_SETUP.md](ENVIRONMENT_SETUP.md#configuration-profiles) | Development, training, and production profiles |

---

## Helper Scripts

### Interactive Setup

```bash
./scripts/setup_env.sh
```

**What it does:**
- Copies `.env.example` → `.env`
- Asks about your use case
- Configures environment automatically
- Downloads model (optional)
- Initializes database (optional)
- Validates configuration

**Best for:** First-time setup, automated configuration

**Time:** ~5-10 minutes

---

### Profile Switching

```bash
./scripts/switch_env_profile.sh dev        # Development profile
./scripts/switch_env_profile.sh training   # Training profile
./scripts/switch_env_profile.sh prod       # Production profile
./scripts/switch_env_profile.sh show       # Show current settings
```

**What it does:**
- Switches between pre-configured profiles
- Updates `.env` with appropriate settings
- Validates configuration

**Best for:** Switching between dev/training/prod

**Time:** <1 minute

---

### Configuration Validation

```bash
./scripts/validate_env.sh
```

**What it does:**
- Checks `.env` file exists
- Validates all variables
- Checks model files
- Verifies tool availability (Rust, Cargo, Node.js, etc.)
- Reports warnings and errors

**Best for:** Troubleshooting, verification

**Time:** ~1 minute

---

## Configuration Profiles

### Development (Recommended for Local Testing)

**Use when:** Developing features, debugging, experimenting

**Key settings:**
- `RUST_LOG=debug,adapteros=trace` - Debug logging
- `AOS_SERVER_PRODUCTION_MODE=false` - All features enabled
- `AOS_SECURITY_JWT_MODE=hs256` - Simple JWT
- `AOS_MODEL_BACKEND=auto` - Auto-select backend

**Setup:**
```bash
./scripts/switch_env_profile.sh dev
```

---

### Training (ML Fine-Tuning)

**Use when:** Training LoRA adapters, GPU experiments, research

**Key settings:**
- `AOS_MODEL_BACKEND=mlx` - MLX GPU backend
- `AOS_MLX_PRECISION=float16` - GPU-optimized precision
- `AOS_MLX_MEMORY_POOL_ENABLED=true` - Memory efficiency
- `RUST_LOG=info,adapteros_lora_mlx_ffi=debug` - Backend debug logs

**Setup:**
```bash
./scripts/switch_env_profile.sh training
```

---

### Production (Secure Serving)

**Use when:** Production inference, compliance requirements, auditing

**Key settings:**
- `AOS_SERVER_PRODUCTION_MODE=true` - Enforced security
- `AOS_MODEL_BACKEND=coreml` - CoreML/ANE acceleration
- `AOS_SECURITY_JWT_MODE=eddsa` - Ed25519 (secure)
- `AOS_SECURITY_PF_DENY=true` - PF deny rules enforced
- `AOS_TELEMETRY_ENABLED=true` - Audit logging

**Setup:**
```bash
./scripts/switch_env_profile.sh prod
```

---

## Environment Variables Reference

### Essential Variables

| Variable | Example | Purpose |
|----------|---------|---------|
| `AOS_MODEL_PATH` | `./models/qwen2.5-7b-mlx` | Where model files are |
| `AOS_MODEL_BACKEND` | `auto` \| `mlx` \| `coreml` \| `metal` | Which backend to use |
| `AOS_SERVER_PORT` | `8080` | API server port |
| `AOS_DATABASE_URL` | `sqlite:var/aos-cp.sqlite3` | Database connection |
| `RUST_LOG` | `debug,adapteros=trace` | Log level |

**Full reference:** See [ENVIRONMENT_SETUP.md#variable-reference](ENVIRONMENT_SETUP.md#variable-reference)

### Grouping by Category

- **Model Configuration:** [ENVIRONMENT_SETUP.md#model-configuration](ENVIRONMENT_SETUP.md#model-configuration)
- **Server Configuration:** [ENVIRONMENT_SETUP.md#server-configuration](ENVIRONMENT_SETUP.md#server-configuration)
- **Database Configuration:** [ENVIRONMENT_SETUP.md#database-configuration](ENVIRONMENT_SETUP.md#database-configuration)
- **Security Configuration:** [ENVIRONMENT_SETUP.md#security-configuration](ENVIRONMENT_SETUP.md#security-configuration)
- **Logging Configuration:** [ENVIRONMENT_SETUP.md#logging-configuration](ENVIRONMENT_SETUP.md#logging-configuration)
- **Memory Configuration:** [ENVIRONMENT_SETUP.md#memory-configuration](ENVIRONMENT_SETUP.md#memory-configuration)
- **Backend Configuration:** [ENVIRONMENT_SETUP.md#backend-configuration](ENVIRONMENT_SETUP.md#backend-configuration)

---

## Common Tasks

### Get Started (First Time)

```bash
# Interactive setup (recommended)
./scripts/setup_env.sh

# Or manual setup
cp .env.example .env
vim .env  # Edit as needed
./scripts/validate_env.sh
```

### Switch Use Case

```bash
# From development to training
./scripts/switch_env_profile.sh training

# Back to development
./scripts/switch_env_profile.sh dev

# Show current settings
./scripts/switch_env_profile.sh show
```

### Download Model

```bash
# Download default model (Qwen 2.5 7B)
./scripts/download_model.sh

# Or manually
huggingface-cli download mlx-community/Qwen2.5-7B-Instruct \
    --include "*.safetensors" "*.json" \
    --local-dir models/qwen2.5-7b-mlx
```

### Initialize Database

```bash
# Create directories
mkdir -p var/artifacts var/bundles var/alerts

# Run migrations
cargo run -p adapteros-orchestrator -- db migrate

# Create default tenant
cargo run -p adapteros-orchestrator -- init-tenant \
    --id default --uid 1000 --gid 1000
```

### Verify Setup

```bash
# Validate all configuration
./scripts/validate_env.sh

# Check current profile
./scripts/switch_env_profile.sh show

# Test backend health
curl http://localhost:8080/healthz
```

---

## File Organization

```
.env.example                          # Configuration template (git-tracked)
.env                                  # Your local config (git-ignored)

docs/
  ├─ ENVIRONMENT_SETUP.md            # Full reference guide
  ├─ ENVIRONMENT_CONFIGURATION_INDEX.md  # This file
  ├─ PRD-CONFIG-001-unified-configuration-system.md  # Design doc
  └─ LOCAL_BUILD.md                  # Build troubleshooting

scripts/
  ├─ setup_env.sh                    # Interactive setup wizard
  ├─ switch_env_profile.sh           # Quick profile switching
  ├─ validate_env.sh                 # Configuration validator
  └─ download_model.sh               # Model downloader

ENVIRONMENT_QUICK_REFERENCE.md        # One-page quick reference
QUICKSTART.md                         # 10-minute quick start
```

---

## Troubleshooting

### Common Issues

| Problem | Solution | Reference |
|---------|----------|-----------|
| `.env` file not found | `cp .env.example .env` | [ENVIRONMENT_SETUP.md#quick-start](ENVIRONMENT_SETUP.md#quick-start) |
| Variables not taking effect | Export first or use `cargo run` | [ENVIRONMENT_SETUP.md#configuration-precedence](ENVIRONMENT_SETUP.md#configuration-precedence) |
| Port already in use | Use different port or kill existing process | [ENVIRONMENT_SETUP.md#troubleshooting](ENVIRONMENT_SETUP.md#troubleshooting) |
| Model not found | Download with `./scripts/download_model.sh` | [ENVIRONMENT_QUICK_REFERENCE.md#download-a-model](../ENVIRONMENT_QUICK_REFERENCE.md#download-a-model) |
| Database errors | Reset with `rm var/aos-cp.sqlite3*` then re-migrate | [ENVIRONMENT_SETUP.md#troubleshooting](ENVIRONMENT_SETUP.md#troubleshooting) |

**Full troubleshooting:** [ENVIRONMENT_SETUP.md#troubleshooting](ENVIRONMENT_SETUP.md#troubleshooting)

---

## Getting Help

### Self-Service

1. **Quick lookup:** [ENVIRONMENT_QUICK_REFERENCE.md](../ENVIRONMENT_QUICK_REFERENCE.md)
2. **Detailed guide:** [ENVIRONMENT_SETUP.md](ENVIRONMENT_SETUP.md)
3. **Validate setup:** `./scripts/validate_env.sh`
4. **Check logs:** `RUST_LOG=debug cargo run --release -p adapteros-server-api`

### Documentation

- **Architecture:** [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md)
- **MLX Backend:** [MLX_INTEGRATION.md](MLX_INTEGRATION.md)
- **CoreML Backend:** [COREML_INTEGRATION.md](COREML_INTEGRATION.md)
- **Configuration System:** [PRD-CONFIG-001-unified-configuration-system.md](PRD-CONFIG-001-unified-configuration-system.md)

---

## Checklist: Environment Setup Complete

- [ ] `.env` file created: `cp .env.example .env`
- [ ] Configuration edited: `vim .env`
- [ ] Configuration validated: `./scripts/validate_env.sh`
- [ ] Model downloaded: `./scripts/download_model.sh`
- [ ] Database initialized: `cargo run -p adapteros-orchestrator -- db migrate`
- [ ] Default tenant created: `cargo run -p adapteros-orchestrator -- init-tenant --id default --uid 1000 --gid 1000`
- [ ] Backend started: `cargo run --release -p adapteros-server-api`
- [ ] Health check passed: `curl http://localhost:8080/healthz`

---

## Related Documentation

- [Quick Start Guide](../QUICKSTART.md) - 10-minute setup
- [Local Build Guide](LOCAL_BUILD.md) - Build issues & solutions
- [Architecture Index](ARCHITECTURE_INDEX.md) - System design
- [Database Reference](DATABASE_REFERENCE.md) - Schema & migrations
- [Feature Flags](FEATURE_FLAGS.md) - Optional features

---

**Last Updated:** 2025-11-23 | **Maintainer:** James KC Auchterlonie

For issues or questions, see the troubleshooting section or check the full [ENVIRONMENT_SETUP.md](ENVIRONMENT_SETUP.md).
