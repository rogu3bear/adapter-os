# AdapterOS Quick Start Guide

Get up and running with AdapterOS inference in 10 minutes.

## Prerequisites

- **macOS with Apple Silicon** (M1/M2/M3/M4 required for Metal backend)
- **Rust 1.75+** (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- **16GB+ RAM** (32GB recommended for Qwen2.5-7B)
- **Git** and **Python 3.9+**

---

## Step 1: Clone and Build

```bash
# Clone repository
git clone <repo-url>
cd adapteros

# Set environment variables
export DATABASE_URL="sqlite://var/aos-cp.sqlite3"
export RUST_LOG="info,adapteros=debug"

# Build release binaries
cargo build --release --bin adapteros-server
cargo build --release --bin adapteros-cli
```

Build time: ~10-15 minutes on Apple Silicon M1/M2.

---

## Step 2: Download Base Model

AdapterOS requires Qwen2.5-7B-Instruct in SafeTensors format.

**Option A: Using HuggingFace CLI (Recommended)**

```bash
# Install HuggingFace CLI
pip install -U "huggingface_hub[cli]"

# Download Qwen2.5-7B model
mkdir -p models
huggingface-cli download Qwen/Qwen2.5-7B-Instruct \
  --local-dir models/qwen2.5-7b \
  --include "model.safetensors" "config.json" "tokenizer.json"
```

**Option B: Manual Download**

1. Go to https://huggingface.co/Qwen/Qwen2.5-7B-Instruct
2. Download these files to `models/qwen2.5-7b/`:
   - `model.safetensors` (~14GB)
   - `config.json`
   - `tokenizer.json`

**Verify download:**

```bash
ls -lh models/qwen2.5-7b/
# Should show:
#   model.safetensors  (~14GB)
#   config.json
#   tokenizer.json
```

---

## Step 3: Initialize Database

```bash
# Create database directory
mkdir -p var

# Run migrations
./target/release/adapteros-cli db migrate

# Initialize default tenant
./target/release/adapteros-cli init-tenant \
  --id default \
  --uid 1000 \
  --gid 1000
```

**Verify database:**

```bash
./target/release/adapteros-cli status db
```

---

## Step 4: Create Sample Adapters

AdapterOS uses `.aos` files for adapters. Create sample adapters:

```bash
# Create adapters directory
mkdir -p adapters

# Option A: Use Python script (recommended)
python3 scripts/create_aos_adapter.py \
  --name code-assistant \
  --output adapters/code-assistant.aos

python3 scripts/create_aos_adapter.py \
  --name creative-writer \
  --output adapters/creative-writer.aos

# Option B: Use Rust packager
cargo run --release --bin aos_packager -- \
  --name readme-writer \
  --output adapters/readme-writer.aos
```

**Verify adapters:**

```bash
ls -lh adapters/
# Should show *.aos files
```

---

## Step 5: Start Server

```bash
# Start server (foreground)
./target/release/adapteros-server

# OR start server (background)
./target/release/adapteros-server &
SERVER_PID=$!
```

The server will:
- Load the base model into memory (~14GB VRAM)
- Initialize Metal kernels
- Listen on Unix socket: `/var/run/adapteros.sock`

**Wait for ready message:**

```
✅ AdapterOS server ready
   Socket: /var/run/adapteros.sock
   Model: qwen2.5-7b
   Adapters loaded: 0
```

---

## Step 6: Run Inference

### Basic Inference (No Adapter)

```bash
./target/release/adapteros-cli infer \
  --prompt "Hello, how are you?" \
  --max-tokens 20
```

**Expected output:**

```
I'm doing well, thank you for asking! How can I help you today?
```

### Inference with Adapter

```bash
./target/release/adapteros-cli infer \
  --adapter code-assistant \
  --prompt "Write a hello world function in Python" \
  --max-tokens 50
```

**Expected output:**

```python
def hello_world():
    """Print hello world message"""
    print("Hello, World!")

hello_world()
```

### Show Trace and Citations

```bash
./target/release/adapteros-cli infer \
  --prompt "Explain k-sparse routing" \
  --max-tokens 100 \
  --require-evidence \
  --show-citations \
  --show-trace
```

---

## Step 7: Test Hot-Swap

```bash
# Swap to creative-writer adapter
./target/release/adapteros-cli adapter-swap \
  --tenant default \
  --add creative-writer \
  --remove code-assistant \
  --commit

# Generate creative content
./target/release/adapteros-cli infer \
  --adapter creative-writer \
  --prompt "Once upon a time" \
  --max-tokens 50
```

**Expected behavior:**
- Swap completes in <100ms
- No service interruption
- Different output style (creative vs code)

---

## Verification Checklist

Run these commands to verify your installation:

```bash
# 1. Server health check
./target/release/adapteros-cli doctor

# 2. List adapters
./target/release/adapteros-cli list-adapters

# 3. System status
./target/release/adapteros-cli status system

# 4. Run end-to-end test
cargo test --test e2e_inference_complete -- --ignored --nocapture

# 5. Run demo script
./scripts/demo_inference.sh
```

All checks should pass ✅

---

## Quick Demo

Run the automated demo to see all features:

```bash
./scripts/demo_inference.sh
```

This will demonstrate:
1. ✅ Basic inference
2. ✅ Adapter-enhanced inference
3. ✅ Hot-swap capability
4. ✅ Evidence-grounded responses
5. ✅ Performance metrics

---

## Troubleshooting

### Server Won't Start

**Symptom:** Server crashes or fails to bind socket

**Solutions:**

```bash
# Check database
./target/release/adapteros-cli db migrate

# Check socket permissions
ls -la /var/run/adapteros.sock
sudo rm /var/run/adapteros.sock  # if stale

# Check Metal support
system_profiler SPDisplaysDataType | grep Metal
```

### Model Not Found

**Symptom:** `Model file not found: models/qwen2.5-7b/model.safetensors`

**Solutions:**

```bash
# Verify download
ls -lh models/qwen2.5-7b/model.safetensors

# Re-download if corrupted
huggingface-cli download Qwen/Qwen2.5-7B-Instruct \
  --local-dir models/qwen2.5-7b \
  --resume-download
```

### Inference Timeout

**Symptom:** `Inference request failed: timeout`

**Solutions:**

```bash
# Increase timeout
./target/release/adapteros-cli infer \
  --prompt "..." \
  --timeout 60000  # 60 seconds

# Reduce max tokens
./target/release/adapteros-cli infer \
  --prompt "..." \
  --max-tokens 10  # smaller generation

# Check memory pressure
./target/release/adapteros-cli status memory
```

### No Adapters Loaded

**Symptom:** `Found 0 adapter(s)`

**Solutions:**

```bash
# List .aos files
ls adapters/*.aos

# Register adapters
./target/release/adapteros-cli register-adapter \
  code-assistant \
  b3:$(cat adapters/code-assistant.aos | b3sum | cut -d' ' -f1) \
  --tier persistent \
  --rank 16

# Verify registration
./target/release/adapteros-cli list-adapters
```

### Metal Initialization Failed

**Symptom:** `Failed to initialize Metal: No Metal device found`

**Cause:** Not running on Apple Silicon or Metal disabled

**Solutions:**

```bash
# Check GPU
system_profiler SPDisplaysDataType | grep -A 5 "Chipset Model"

# If Linux/x86, build without Metal
cargo build --release --no-default-features
```

### Memory Pressure Errors

**Symptom:** `System under pressure, retry in 30s`

**Solutions:**

```bash
# Check memory usage
./target/release/adapteros-cli status memory

# Evict cold adapters
./target/release/adapteros-cli maintenance evict-cold

# Reduce model size or use quantization
# (see docs/QUANTIZATION.md)
```

---

## Custom Adapter Training Workflow

Create your own custom adapter using a dataset:

### Step 1: Prepare Training Data

Create a JSONL file (`training.jsonl`) with input-output pairs:

```jsonl
{"input": "What is machine learning?", "target": "Machine learning is a subset of artificial intelligence focused on learning from data."}
{"input": "Explain neural networks", "target": "Neural networks are computing systems inspired by biological neural networks that form an animal brain."}
{"input": "What is deep learning?", "target": "Deep learning is a subset of machine learning using neural networks with multiple layers."}
```

### Step 2: Upload Dataset

```bash
curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=my-training-data" \
  -F "description=Custom training dataset" \
  -F "format=jsonl" \
  -F "file=@training.jsonl"
```

Save the returned `dataset_id` from the response.

### Step 3: Train Custom Adapter

```bash
# Start training with your dataset
./target/release/aosctl train \
  --dataset-id <dataset_id> \
  --output adapters/my-custom.aos \
  --rank 24 \
  --epochs 3 \
  --learning-rate 0.0001
```

Training will take 5-15 minutes depending on your dataset size and hardware.

### Step 4: Test Your Adapter

```bash
./target/release/aosctl infer \
  --adapter my-custom \
  --prompt "What is machine learning?" \
  --max-tokens 50
```

**Expected output:**

```
Machine learning is a branch of artificial intelligence that enables systems
to learn and improve from experience without explicit programming...
```

### Complete Dataset Workflow Example

Automate the entire workflow:

```bash
#!/bin/bash

# Create training data
cat > training.jsonl << 'EOF'
{"input": "Explain Python", "target": "Python is a high-level programming language known for simplicity and readability."}
{"input": "What is Rust?", "target": "Rust is a systems programming language offering memory safety without garbage collection."}
{"input": "Tell me about JavaScript", "target": "JavaScript is a versatile language primarily used for web development and increasingly for backend services."}
EOF

# Upload dataset
echo "Uploading dataset..."
RESPONSE=$(curl -s -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=programming-languages" \
  -F "description=Programming language explanations" \
  -F "format=jsonl" \
  -F "file=@training.jsonl")

DATASET_ID=$(echo "$RESPONSE" | grep -o '"dataset_id":"[^"]*' | cut -d'"' -f4)
echo "Dataset ID: $DATASET_ID"

# Validate dataset
echo "Validating dataset..."
curl -s -X POST http://localhost:8080/v1/datasets/$DATASET_ID/validate | jq .

# Preview dataset
echo "Dataset preview:"
curl -s "http://localhost:8080/v1/datasets/$DATASET_ID/preview?limit=3" | jq .

# Train adapter
echo "Training custom adapter..."
./target/release/aosctl train \
  --dataset-id $DATASET_ID \
  --output adapters/lang-explainer.aos \
  --rank 16 \
  --epochs 2

# Test inference
echo "Testing trained adapter..."
./target/release/aosctl infer \
  --adapter lang-explainer \
  --prompt "Explain Go programming language" \
  --max-tokens 50

echo "Done!"
```

**For detailed dataset documentation**, see `docs/USER_GUIDE_DATASETS.md`

---

## Next Steps

### Explore Features

- **Datasets:** `docs/USER_GUIDE_DATASETS.md` - Complete dataset guide
- **Training:** `docs/TRAINING.md` - Advanced training techniques
- **Policy:** `docs/POLICY.md` - Configure governance rules
- **Federation:** `docs/FEDERATION.md` - Multi-node setup
- **Telemetry:** `docs/TELEMETRY.md` - Audit trail and metrics

### Advanced Usage

```bash
# Create custom adapter from code
./target/release/adapteros-cli train \
  --input src/ \
  --output adapters/custom.aos \
  --rank 32

# Replay deterministic bundle
./target/release/adapteros-cli replay \
  var/bundles/baseline.ndjson

# Verify cross-host determinism
./target/release/adapteros-cli node-verify --all
```

### Web UI (Optional)

```bash
# Build UI
cd ui && pnpm install && pnpm build

# Access dashboard
open http://localhost:8080
```

### Documentation

- **Architecture:** `docs/ARCHITECTURE_INDEX.md`
- **API Reference:** `docs/API.md`
- **Developer Guide:** `CLAUDE.md`
- **Contributing:** `CONTRIBUTING.md`

---

## Performance Benchmarks

On Apple M1 Max (32GB RAM):

| Operation | Latency | Throughput |
|-----------|---------|------------|
| Model loading | ~8s | - |
| Adapter hot-swap | ~50ms | - |
| Token generation | ~10ms/token | ~100 tokens/s |
| Router decision | <1ms | - |
| Evidence retrieval | ~20ms | - |

*Your results may vary based on hardware*

---

## Getting Help

- **Issues:** https://github.com/anthropics/adapteros/issues
- **Discord:** [join link]
- **Documentation:** `docs/` directory
- **Examples:** `examples/` directory

---

## What's Next?

You now have a working AdapterOS installation! Try:

1. ✅ Generate text with different adapters
2. ✅ Create your own custom adapter
3. ✅ Test deterministic replay
4. ✅ Explore the web UI
5. ✅ Read the architecture docs

**Happy inferencing! 🚀**

---

**Last Updated:** 2025-01-19
**Version:** Alpha v0.01-1
**Platform:** macOS 13.0+ with Apple Silicon
