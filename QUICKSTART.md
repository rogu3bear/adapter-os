# adapterOS Quick Start for macOS

Get the full adapterOS UX running in 10 minutes.

**Requirements:** Apple Silicon Mac (M1/M2/M3/M4), 16GB+ RAM, macOS 14+

---

## 1. Prerequisites

```bash
# Install Xcode Command Line Tools
xcode-select --install

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install Node.js and pnpm
brew install node@20
npm install -g pnpm

# Optional but recommended
brew install b3sum sqlite3
```

Verify setup:
```bash
rustc --version    # 1.75+
node --version     # 20+
pnpm --version     # 8+
```

---

## 2. Clone and Build

```bash
git clone https://github.com/rogu3bear/adapter-os.git
cd adapter-os

# Build release binaries
cargo build --release --workspace
```

---

## 3. Download a Model

```bash
# Download Qwen 2.5 7B (4-bit quantized, ~3.8GB)
./aosctl models seed

# Or specify custom model path:
# ./aosctl models seed --model-path ./var/models/qwen2.5-7b-4bit
```

The model will be downloaded to `models/qwen2.5-7b-instruct-4bit-mlx/`.

---

## 4. Initialize Database

```bash
# Create data directories
mkdir -p var/artifacts var/bundles var/alerts

# Copy environment template
cp .env.example .env

# (Optional) Review/edit environment configuration
# See docs/ENVIRONMENT_SETUP.md for detailed configuration guide
# vim .env

# Build CLI and create symlink
cargo build --release -p adapteros-cli --features tui
ln -sf target/release/aosctl ./aosctl

# Run database migrations
./aosctl db migrate

# Initialize default tenant
./aosctl init-tenant --id default --uid 1000 --gid 1000
```

---

## 5. Start the System

**Option A (Canonical): Unified `./start`**
```bash
./start               # backend + UI, health waits, drift checks
./start backend       # backend only (UI skipped), worker optional
./start status        # show status without starting
./start --verify-chat # optional chat response verification (after /readyz)
```
- Uses `scripts/service-manager.sh` under the hood; no parallel boot path
- Worker starts only if binaries + manifest are present; otherwise skipped
- If `AOS_SERVER_PORT` is unset, `./start` reads `server.port` from `configs/cp.toml` (or `AOS_CONFIG`/`AOS_CONFIG_PATH`)
- Chat verification requires `AOS_AUTH_TOKEN`/`AOS_TOKEN` or dev bypass (`AOS_DEV_NO_AUTH=1` in debug builds)

**Option B (Advanced / Manual — bypasses guardrails and health waits)**

Terminal 1 - Backend:
```bash
export AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-instruct-4bit-mlx
export DATABASE_URL=sqlite://var/aos-cp.sqlite3
export RUST_LOG=info

cargo run --release -p adapteros-server -- --config configs/cp.toml
```

Terminal 2 - UI (Development):
```bash
cd crates/adapteros-ui
trunk serve
```

**Note:** For production, the UI is served from the `static/` directory by the backend (via `./start`). The development UI runs on port 3200, while the production UI is served on port 8080 along with the API.

**Legacy scripts (deprecated; prompt with default No before continuing):**
- `scripts/run_complete_system.sh` (redirects to `./start`)
- `scripts/bootstrap_integration_test.sh` / `scripts/bootstrap_with_checkpoints.sh` (legacy bootstrap flows)
- Use only if `./start` is unavailable and you explicitly need the legacy behavior.

---

### Golden Path (Scripts)

Use these scripts for the verified end-to-end loop:

- `./scripts/dev-up.sh` - Start backend + UI (health checks included)
- `./scripts/worker-up.sh` - Start worker and verify registration
- `./scripts/golden_path_adapter_chat.sh` - Dataset -> training -> adapter -> chat response

---

## 6. Access the UI

**Development Mode:**
- UI dev server: http://localhost:3200 (when running `trunk serve` in `crates/adapteros-ui`)

**Production Mode:**
- UI served by backend: http://localhost:8080 (when running `./start`)

**Key Pages:**

| Page | URL | Purpose |
|------|-----|---------|
| Dashboard | `/` | System overview |
| Chat | `/chat` | Run inference and stream responses |
| Adapters | `/adapters` | Manage adapters |
| Training | `/training` | Train new adapters |
| Monitoring | `/monitoring` | Performance and health monitoring |

---

## 7. Load a Model

The model loads automatically on first inference. To verify the server is ready:

```bash
curl http://localhost:8080/healthz
# {"status":"healthy",...}
```

Or via UI: Navigate to the Dashboard to see system health status.

---

## 8. Run Inference

**Via UI:**
1. Go to `/chat`
2. Enter a prompt (e.g., "Write a hello world function in Rust:")
3. Configure options (temperature, max tokens)
4. Click "Generate"

**Via API:**
```bash
curl -X POST http://localhost:8080/v1/infer \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Write a hello world function in Rust:",
    "max_tokens": 200,
    "temperature": 0.7
  }'
```

**Streaming inference:**
```bash
curl -X POST http://localhost:8080/v1/infer \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Explain async/await in Rust:",
    "max_tokens": 300,
    "stream": true
  }'
```

---

## 9. Train an Adapter

**Step 1: Prepare training data** (JSONL format):

Create `training_data.jsonl`:
```jsonl
{"prompt": "What is Rust?", "completion": "Rust is a systems programming language..."}
{"prompt": "Explain ownership", "completion": "Ownership is Rust's memory management..."}
{"prompt": "What are lifetimes?", "completion": "Lifetimes are Rust's way of tracking..."}
```

### Training dataset contract

adapterOS accepts JSONL datasets in either schema:

- Supervised: `{"prompt": "string", "completion": "string"}`
- Raw: `{"text": "string"}`

Raw framing policy: `raw_continuation_v1` with constants:

- `MAX_INPUT_TOKENS=256`
- `MAX_TARGET_TOKENS=128`
- `STRIDE_TOKENS=256`

Dataset fixtures created by scripts land in `var/datasets/`:

- `scripts/build-codebase-dataset.sh` -> `var/datasets/codebase/`
- `scripts/train-codebase-adapter.sh` -> `var/datasets/codebase/training.jsonl`

**Step 2: Train via UI:**
1. Go to `/training`
2. Click "New Training Job"
3. Upload your JSONL dataset
4. Configure hyperparameters:
   - Rank: 16 (default)
   - Alpha: 32 (default)
   - Learning Rate: 1e-4
   - Epochs: 3
5. Start training
6. Monitor progress on the Training page

**Or via API:**
```bash
# Upload dataset
curl -X POST http://localhost:8080/v1/datasets/upload \
  -H "Content-Type: multipart/form-data" \
  -F "file=@training_data.jsonl" \
  -F "name=my-dataset"

# Start training
curl -X POST http://localhost:8080/v1/training/start \
  -H "Content-Type: application/json" \
  -d '{
    "dataset_id": "my-dataset",
    "name": "my-adapter",
    "rank": 16,
    "alpha": 32,
    "epochs": 3
  }'

# Check training status
curl http://localhost:8080/v1/training/jobs
```

---

## 10. Use the Trained Adapter

Once training completes, the adapter is automatically registered.

**Load and use via UI:**
1. Go to `/inference`
2. Select your adapter from the dropdown
3. Enter a prompt and generate

**Or via API:**
```bash
# Load the adapter
curl -X POST http://localhost:8080/v1/adapters/my-adapter/load

# Run inference with the adapter
curl -X POST http://localhost:8080/v1/infer \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Your prompt here",
    "max_tokens": 200,
    "adapters": ["my-adapter"]
  }'
```

---

## Performance Expectations

| Hardware | Inference Latency | Token Generation | Model Load |
|----------|-------------------|------------------|------------|
| M1 (16GB) | ~1ms/token | ~1,000 tok/sec | 5-8 sec |
| M2/M3 Pro | ~0.6ms/token | ~1,600 tok/sec | 3-5 sec |
| M4 Max (48GB) | ~0.39ms/token | ~2,500 tok/sec | 2-3 sec |

Adapter hot-swap: <100ms

---

## Troubleshooting

**Port in use:**
```bash
lsof -i :8080
lsof -i :5173
pkill -f adapteros-server
```

**Model not found:**
```bash
ls -la models/
echo $AOS_MLX_FFI_MODEL
./aosctl models seed  # Re-seed if needed
```

**Database errors:**
```bash
rm var/aos-cp.sqlite3
./aosctl db migrate
```

**Memory issues:**
- Use a smaller model: `./aosctl models seed --model-path ./var/models/qwen2.5-3b-4bit`
- Reduce concurrent requests in `configs/cp.toml`

**UI won't connect:**
```bash
# Check backend is running
curl http://localhost:8080/healthz

# Restart UI dev server
cd ui && pnpm dev
```

---

## Next Steps

- **[Full Architecture](docs/ARCHITECTURE.md)** - System design
- **[MLX Guide](docs/MLX_GUIDE.md)** - Backend details
- **[Training Guide](docs/TRAINING.md)** - Advanced training
- **[REST API Reference](docs/API_REFERENCE.md)** - All endpoints
- **[Deployment Guide](docs/DEPLOYMENT.md)** - Detailed setup

---

## See Also

- [docs/CONFIGURATION.md](docs/CONFIGURATION.md) - Configuration and environment setup
- [docs/COREML_BACKEND.md](docs/COREML_BACKEND.md) - CoreML backend with ANE acceleration
- [docs/TRAINING.md](docs/TRAINING.md) - Training guide
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) - Detailed architecture patterns
- [docs/DATABASE.md](docs/DATABASE.md) - Database schema reference

---

**Built for Apple Silicon** | Copyright 2025 JKCA / James KC Auchterlonie

MLNavigator Inc January 2026.
