# Getting Started with AdapterOS

> **This is the canonical getting started guide for basic backend setup.** For complete setup with UI, see [QUICKSTART.md](QUICKSTART.md).

Get AdapterOS running in 5 minutes on your Apple Silicon Mac.

---

## Prerequisites

### System Requirements

| Requirement | Minimum | Notes |
|-------------|---------|-------|
| **macOS** | 13.0+ (Ventura) | Apple Silicon (M1/M2/M3/M4) required |
| **Rust** | stable | Managed via `rust-toolchain.toml` |
| **SQLite** | 3.35+ | Ships with macOS |
| **Disk Space** | 2GB | For build artifacts |

### Install Rust

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify installation
rustc --version
cargo --version
```

### Optional: MLX for Production Inference

```bash
# Required only for real model inference
brew install mlx
```

---

## Quick Start (5 Minutes)

### 1. Clone and Build

```bash
# Clone the repository
git clone https://github.com/rogu3bear/adapter-os.git
cd adapter-os

# Build CLI and create symlink
cargo build --release -p adapteros-cli
ln -sf target/release/aosctl ./aosctl
```

### 2. Run Database Migrations

```bash
./aosctl db migrate
```

Expected output:
```
Applied migrations successfully
```

### 3. Seed a Test Model

For testing and development, create a minimal mock model:

```bash
# Create minimal test model directory
mkdir -p var/models/tiny-test
cat > var/models/tiny-test/config.json << 'EOF'
{"model_type": "llama", "hidden_size": 256, "num_attention_heads": 4}
EOF
cat > var/models/tiny-test/tokenizer.json << 'EOF'
{}
EOF
cat > var/models/tiny-test/tokenizer_config.json << 'EOF'
{}
EOF

# Seed the model
./aosctl models seed --model-path var/models/tiny-test

# Verify model was seeded
./aosctl models list
```

For production use, download a real model instead:

```bash
# Download Qwen 2.5 7B MLX format (~3.8GB)
huggingface-cli download mlx-community/Qwen2.5-7B-Instruct \
    --include "*.safetensors" "*.json" \
    --local-dir var/models/qwen2.5-7b-mlx

./aosctl models seed --model-path var/models/qwen2.5-7b-mlx
```

### 4. Start the Server

```bash
# Start dev server with auth disabled (recommended for first run)
AOS_DEV_NO_AUTH=1 cargo run -p adapteros-server -- --config configs/cp.toml
```

The server starts on port 8080 by default.

### 5. Verify Installation

Open a new terminal and run:

```bash
# Check health endpoints
curl -s http://127.0.0.1:8080/healthz && echo " OK"
curl -s http://127.0.0.1:8080/readyz && echo " OK"

# Check system status
./aosctl status
```

---

## Verify Your Installation

AdapterOS provides built-in diagnostics commands:

### System Health Check

```bash
./aosctl doctor
```

The doctor command checks:
- Database connectivity
- Configuration validity
- Required directories
- Backend availability

### Pre-flight Readiness

```bash
./aosctl preflight
```

The preflight command validates:
- All migrations applied
- Models seeded
- Configuration complete
- Network/port availability

---

## Development Configuration

### Auth-Disabled Mode

For local development, bypass authentication:

```bash
AOS_DEV_NO_AUTH=1 cargo run -p adapteros-server -- --config configs/cp.toml
```

Or set in the config file (debug builds only):

```toml
# configs/cp.toml
[security]
dev_login_enabled = true
```

### Custom Port

```bash
# Use a different port
AOS_SERVER_PORT=9000 cargo run -p adapteros-server -- --config configs/cp.toml
```

### Mock Backend (No GPU Required)

```bash
# Use mock backend for testing without real inference
AOS_BACKEND=mock cargo run -p adapteros-server -- --config configs/cp.toml
```

---

## Directory Structure

After setup, your workspace looks like:

```
adapter-os/
├── aosctl                    # CLI symlink
├── configs/cp.toml           # Main configuration
├── var/
│   ├── aos-cp.sqlite3        # SQLite database
│   ├── models/               # Seeded models
│   │   └── tiny-test/        # Test model
│   ├── adapters/             # LoRA adapters
│   ├── logs/                 # Log files
│   └── artifacts/            # Build artifacts
└── target/release/           # Built binaries
```

---

## Next Steps

### Learn the CLI

```bash
# List all commands
./aosctl --help

# Explore specific commands
./aosctl adapter --help
./aosctl models --help
./aosctl db --help
```

### Run Tests

```bash
# Run unit tests
cargo test --workspace

# Run the happy path smoke test
./scripts/test/smoke_happy_path.sh
```

### Explore Documentation

- **[QUICKSTART.md](QUICKSTART.md)** - Extended quick start with UI setup
- **[ARCHITECTURE.md](ARCHITECTURE.md)** - System architecture overview
- **[CLI_GUIDE.md](CLI_GUIDE.md)** - Complete CLI reference
- **[CONFIGURATION.md](CONFIGURATION.md)** - Configuration reference
- **[TROUBLESHOOTING.md](TROUBLESHOOTING.md)** - Common issues and solutions

### Start Building

```bash
# Register an adapter
./aosctl register-adapter my-tenant/engineering/code-review/v1 \
    --tier persistent --rank 16

# List adapters
./aosctl adapter list

# Create an adapter stack
./aosctl stack list
```

---

## Common Issues

### "Database not found"

Run migrations first:
```bash
./aosctl db migrate
```

### "Connection refused" on localhost:8080

Ensure the server is running:
```bash
AOS_DEV_NO_AUTH=1 cargo run -p adapteros-server -- --config configs/cp.toml
```

### "Permission denied" errors

Check directory permissions:
```bash
mkdir -p var/logs var/models var/adapters var/artifacts
chmod 755 var var/*
```

### Build errors with MLX

MLX is optional for basic testing. Use mock backend:
```bash
AOS_BACKEND=mock cargo run -p adapteros-server -- --config configs/cp.toml
```

---

## Smoke Test

Verify your complete setup with the smoke test:

```bash
./scripts/test/smoke_happy_path.sh
```

This script:
1. Builds the CLI
2. Runs database migrations
3. Seeds a test model
4. Starts the server
5. Tests health endpoints
6. Cleans up automatically

Expected output:
```
[1/6] Building CLI...
[2/6] Running migrations...
[3/6] Seeding test model...
[4/6] Starting server...
[5/6] Testing endpoints...
[6/6] Cleanup...
SMOKE TEST PASSED
```

---

## Getting Help

- **Documentation**: Browse the `/docs` directory
- **API Reference**: Run `cargo doc --open`
- **GitHub Issues**: [Report bugs or request features](https://github.com/rogu3bear/adapter-os/issues)
- **CLI Help**: Run `./aosctl --help` or `./aosctl <command> --help`

---

**Built for Apple Silicon** | Deterministic ML Inference | Zero Network Egress
