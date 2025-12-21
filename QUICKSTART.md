# AdapterOS Quick Start for macOS

Get the full AdapterOS UX running in 10 minutes.

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
./scripts/download_model.sh

# Or specify options:
# ./scripts/download_model.sh --size 3b    # Smaller 3B model
# ./scripts/download_model.sh --size 0.5b  # Tiny model for testing
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

# Run database migrations
cargo run --release -p adapteros-orchestrator -- db migrate

# Initialize default tenant
cargo run --release -p adapteros-orchestrator -- init-tenant \
    --id default --uid 1000 --gid 1000
```

---

## 5. Start the System

**Option A (Canonical): Unified `./start`**
```bash
./start               # backend + UI, health waits, drift checks
./start backend       # backend only (UI skipped), worker optional
./start status        # show status without starting
```
- Uses `scripts/service-manager.sh` under the hood; no parallel boot path
- Worker starts only if binaries + manifest are present; otherwise skipped

**Option B (Advanced / Manual — bypasses guardrails and health waits)**

Terminal 1 - Backend:
```bash
export AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-instruct-4bit-mlx
export DATABASE_URL=sqlite://var/aos-cp.sqlite3
export RUST_LOG=info

cargo run --release -p adapteros-server-api
```

Terminal 2 - UI:
```bash
cd ui
pnpm install
pnpm dev
```

**Legacy scripts (deprecated; prompt with default No before continuing):**
- `scripts/run_complete_system.sh` (redirects to `./start`)
- `scripts/bootstrap_integration_test.sh` / `scripts/bootstrap_with_checkpoints.sh` (legacy bootstrap flows)
- Use only if `./start` is unavailable and you explicitly need the legacy behavior.

---

## 6. Access the UI

Open http://localhost:3200 in your browser.

**Key Pages:**

| Page | URL | Purpose |
|------|-----|---------|
| Dashboard | `/dashboard` | System overview |
| Inference | `/inference` | Run inference |
| Adapters | `/adapters` | Manage adapters |
| Training | `/training` | Train new adapters |
| Metrics | `/metrics` | Performance monitoring |

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
1. Go to `/inference`
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
./scripts/download_model.sh  # Re-download if needed
```

**Database errors:**
```bash
rm var/aos-cp.sqlite3
cargo run --release -p adapteros-orchestrator -- db migrate
```

**Memory issues:**
- Use a smaller model: `./scripts/download_model.sh --size 3b`
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
- **[MLX Integration](docs/MLX_INTEGRATION.md)** - Backend details
- **[Training Guide](docs/training/aos_adapters.md)** - Advanced training
- **[REST API Reference](AGENTS.md#rest-api-reference)** - All endpoints
- **[Complete System Guide](docs/QUICKSTART_COMPLETE_SYSTEM.md)** - Detailed setup

---

## See Also

- [AGENTS.md](AGENTS.md) - Developer quick reference guide
- [docs/LOCAL_BUILD.md](docs/LOCAL_BUILD.md) - Local build guide with troubleshooting
- [docs/FEATURE_FLAGS.md](docs/FEATURE_FLAGS.md) - Feature flag reference
- [docs/COREML_INTEGRATION.md](docs/COREML_INTEGRATION.md) - CoreML backend with ANE acceleration
- [QUICKSTART_GPU_TRAINING.md](QUICKSTART_GPU_TRAINING.md) - GPU training quick start
- [docs/ARCHITECTURE.md#core-concepts](docs/ARCHITECTURE.md#core-concepts) - Detailed architecture patterns
- [docs/DATABASE_REFERENCE.md](docs/DATABASE_REFERENCE.md) - Database schema reference

---

**Built for Apple Silicon** | Copyright 2025 JKCA / James KC Auchterlonie

MLNavigator Inc 2025-12-06.
