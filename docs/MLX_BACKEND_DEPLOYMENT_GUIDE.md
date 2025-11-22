# MLX Backend Deployment Guide

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2025-11-22
**Status:** Production Deployment Ready

---

## Table of Contents
1. [Overview](#overview)
2. [System Requirements](#system-requirements)
3. [Installation & Building](#installation--building)
4. [Configuration](#configuration)
5. [Production Deployment](#production-deployment)
6. [Monitoring & Maintenance](#monitoring--maintenance)
7. [Troubleshooting](#troubleshooting)
8. [Performance Tuning](#performance-tuning)

---

## Overview

The MLX backend is a production-ready GPU acceleration layer for AdapterOS supporting research and training workloads on Apple Silicon. It integrates seamlessly with the multi-backend architecture and includes enterprise-grade resilience, health monitoring, and deterministic seeding.

### Key Features

| Feature | Status | Details |
|---------|--------|---------|
| **Model Loading** | ✅ Production | FFI-based loading from directory or buffer |
| **Inference** | ✅ Production | Forward passes, hidden state extraction |
| **Text Generation** | ✅ Production | Temperature, top-k, top-p sampling |
| **Determinism** | ✅ Production | HKDF-seeded RNG for reproducibility |
| **Hot-Swap** | ✅ Production | Live adapter loading/unloading |
| **Health Monitoring** | ✅ Production | Circuit breaker, auto-recovery |
| **Memory Management** | ✅ Production | Unified memory tracking, GC hints |

### Architecture Position

```
AdapterOS Multi-Backend Strategy
├── CoreML (ANE acceleration) - Primary/Production
├── MLX (GPU acceleration) - Research/Training ← You are here
└── Metal (Fallback) - Legacy support
```

---

## System Requirements

### Hardware
- **Processor:** Apple Silicon (M1, M2, M3, M4 or compatible)
- **Memory:** Minimum 8GB unified memory (16GB+ recommended for models >7B)
- **Storage:** 50GB free for base OS + models

### Software
- **macOS:** 12.0 or later
- **MLX C++ Library:** Version 0.1.0 or later

### Build Environment
```bash
# Xcode command line tools
xcode-select --install

# Homebrew (for MLX installation)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Rust (1.70+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update
```

---

## Installation & Building

### Step 1: Install MLX Library

```bash
# Via Homebrew (recommended)
brew install mlx

# Verify installation
ls -la /opt/homebrew/include/mlx/
ls -la /opt/homebrew/lib/libmlx*
```

### Step 2: Clone & Build AdapterOS

```bash
# Clone repository
git clone https://github.com/yourusername/aos.git
cd aos

# Build workspace libraries
cargo build --workspace --lib --release

# Build with MLX backend enabled
cargo build -p adapteros-lora-mlx-ffi --features real-mlx --release
```

### Step 3: Build CLI with MLX Support

```bash
# Build CLI tool
cargo build -p adapteros-orchestrator --release

# Verify MLX availability
./target/release/aosctl server-info | grep -A5 "Backend Status"
```

### Build Environment Variables

```bash
# Standard (auto-detects Homebrew)
cargo build -p adapteros-lora-mlx-ffi --features real-mlx

# Custom MLX installation
export MLX_INCLUDE_DIR=/usr/local/include
export MLX_LIB_DIR=/usr/local/lib
cargo build -p adapteros-lora-mlx-ffi --features real-mlx

# Verify real build (not stub)
# Look for: "MLX FFI build: REAL" in build output
```

---

## Configuration

### Configuration File Structure

Create `configs/mlx.toml`:

```toml
# MLX Backend Configuration
[mlx]
# Enable MLX backend
enabled = true

# Model directory path
model_path = "./models/qwen2.5-7b-mlx"

# Backend selection (metal|mlx|coreml|auto)
default_backend = "mlx"

# Memory configuration (MB)
max_memory_mb = 16000
min_free_memory_mb = 1000
gc_threshold_mb = 2000

# Resilience configuration
[mlx.resilience]
max_consecutive_failures = 5
circuit_breaker_timeout_secs = 300
enable_stub_fallback = false
health_check_interval_secs = 60

# Performance tuning
[mlx.performance]
batch_size = 16
prefetch_adapters = true
enable_kv_cache = true
cache_warmup_tokens = 512

# Determinism settings
[mlx.determinism]
# Enable HKDF seeding (required for reproducible results)
use_hkdf_seeding = true
# Base seed (typically model hash)
base_seed = "automatic"  # Or explicit hex value
```

### Environment Variables

```bash
# Model path override
export AOS_MLX_FFI_MODEL="./models/qwen2.5-7b-mlx"

# Logging
export RUST_LOG=info,adapteros_lora_mlx_ffi=debug

# Build configuration
export MLX_INCLUDE_DIR=/opt/homebrew/include
export MLX_LIB_DIR=/opt/homebrew/lib

# Force stub build (testing)
export MLX_FORCE_STUB=1

# Memory settings
export AOS_MLX_MAX_MEMORY_MB=16000
export AOS_MLX_GC_THRESHOLD_MB=2000
```

---

## Production Deployment

### Pre-Deployment Checklist

- [ ] MLX C++ library installed and verified
- [ ] Build completed with real MLX (check for "MLX FFI build: REAL")
- [ ] All unit tests passing: `cargo test -p adapteros-lora-mlx-ffi`
- [ ] Integration tests passing: `cargo test -p adapteros-lora-worker --test '*backend*'`
- [ ] Model files prepared and verified
- [ ] Database migrations applied: `aosctl db migrate`
- [ ] Configuration files in place
- [ ] Monitoring/telemetry configured
- [ ] Backup strategy in place

### Deployment Steps

**Step 1: Prepare Environment**
```bash
# Create model directory
mkdir -p /data/models/base

# Copy model files
cp -r models/qwen2.5-7b-mlx /data/models/base/

# Set permissions
chmod -R 755 /data/models/base

# Verify structure
ls -la /data/models/base/qwen2.5-7b-mlx/
# Should show: config.json, model.safetensors, tokenizer.json
```

**Step 2: Initialize Database**
```bash
# Run migrations
export DATABASE_URL="sqlite:///data/aos.db"
./target/release/aosctl db migrate

# Create system tenant
./target/release/aosctl init-tenant \
  --id system \
  --uid 1000 \
  --gid 1000 \
  --isolation-level=strict
```

**Step 3: Start Server with MLX**
```bash
# Production mode (requires UDS socket)
export AOS_MLX_FFI_MODEL="/data/models/base/qwen2.5-7b-mlx"
export RUST_LOG="info,adapteros_lora_mlx_ffi=info"

./target/release/aosctl serve \
  --tenant system \
  --backend mlx \
  --model-path /data/models/base/qwen2.5-7b-mlx \
  --uds-socket /var/run/aos/kernel.sock \
  --production-mode
```

**Step 4: Verify Deployment**
```bash
# Health check
curl http://localhost:8080/healthz

# Backend status
curl http://localhost:8080/healthz/backend | jq .

# Model info
curl http://localhost:8080/v1/models | jq .

# Test inference (through API)
curl -X POST http://localhost:8080/v1/infer \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "prompt": "Hello",
    "max_tokens": 10,
    "adapters": []
  }' | jq .
```

### Systemd Service Template

Create `/etc/systemd/system/aos-mlx.service`:

```ini
[Unit]
Description=AdapterOS with MLX Backend
After=network.target
Wants=aos-db.service

[Service]
Type=simple
User=aos
Group=aos
WorkingDirectory=/opt/aos

Environment="DATABASE_URL=sqlite:////var/lib/aos/aos.db"
Environment="AOS_MLX_FFI_MODEL=/data/models/base/qwen2.5-7b-mlx"
Environment="RUST_LOG=info,adapteros_lora_mlx_ffi=info"
Environment="RUST_BACKTRACE=1"

ExecStart=/opt/aos/bin/aosctl serve \
  --tenant system \
  --backend mlx \
  --model-path /data/models/base/qwen2.5-7b-mlx \
  --uds-socket /var/run/aos/kernel.sock \
  --production-mode

# Restart policy
Restart=on-failure
RestartSec=10s
StartLimitInterval=60s
StartLimitBurst=3

# Resource limits
MemoryLimit=24G
MemoryAccounting=true
CPUAccounting=true

# Hardening
PrivateTmp=yes
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/lib/aos /var/run/aos /data/models

[Install]
WantedBy=multi-user.target
```

Enable and start service:
```bash
sudo systemctl daemon-reload
sudo systemctl enable aos-mlx.service
sudo systemctl start aos-mlx.service

# Monitor
sudo systemctl status aos-mlx.service
sudo journalctl -u aos-mlx.service -f
```

---

## Monitoring & Maintenance

### Health Monitoring

```rust
use adapteros_lora_mlx_ffi::MLXFFIModel;

// Check model health
let model = MLXFFIModel::load("./models/qwen2.5-7b-mlx")?;

if let Some(health) = model.health_status() {
    println!("Operational: {}", health.operational);
    println!("Failures: {}", health.consecutive_failures);
    println!("Circuit breaker: {:?}", health.circuit_breaker);
    println!("Last success: {:?}", health.last_success);
}

// Check memory usage
let stats = memory::stats();
println!("{}", memory::format_stats(&stats));

// Check if memory exceeds threshold
if memory::exceeds_threshold(12000.0) {  // 12GB
    memory::gc_collect();
}
```

### Metrics Collection

```bash
# Prometheus metrics endpoint
curl http://localhost:8080/v1/metrics | grep mlx_

# Key metrics
mlx_backend_health_operational       # 0/1
mlx_backend_requests_total           # Counter
mlx_backend_request_duration_seconds  # Histogram
mlx_backend_memory_usage_bytes        # Gauge
mlx_backend_adapter_count             # Gauge
mlx_backend_circuit_breaker_state     # 0=Closed, 1=Open, 2=HalfOpen
```

### Alerting Rules

```yaml
# prometheus/rules.yml
groups:
  - name: mlx_backend
    rules:
      - alert: MLXCircuitBreakerOpen
        expr: mlx_backend_circuit_breaker_state == 1
        for: 5m
        annotations:
          summary: "MLX backend circuit breaker is open"

      - alert: MLXHighMemoryUsage
        expr: mlx_backend_memory_usage_bytes > 15 * 1024 * 1024 * 1024
        for: 10m
        annotations:
          summary: "MLX backend memory usage > 15GB"

      - alert: MLXHighFailureRate
        expr: rate(mlx_backend_requests_failed[5m]) > 0.05
        for: 10m
        annotations:
          summary: "MLX backend failure rate > 5%"
```

### Maintenance Tasks

**Daily**
```bash
# Check health
curl http://localhost:8080/healthz/backend

# Monitor logs
journalctl -u aos-mlx.service --since "1h ago" | grep -i error
```

**Weekly**
```bash
# Check memory fragmentation
sqlite3 /var/lib/aos/aos.db "PRAGMA integrity_check;"

# Verify model files
md5sum /data/models/base/qwen2.5-7b-mlx/*.safetensors
```

**Monthly**
```bash
# Backup configuration
tar -czf aos-config-$(date +%Y%m%d).tar.gz /etc/aos/ /etc/systemd/system/aos-*.service

# Review metrics and performance trends
# (check monitoring dashboard)

# Update MLX library
brew upgrade mlx
cargo build -p adapteros-lora-mlx-ffi --features real-mlx --release
```

---

## Troubleshooting

### Build Issues

| Problem | Cause | Solution |
|---------|-------|----------|
| `linking with 'cc' failed` | MLX library not found | Set `MLX_INCLUDE_DIR` and `MLX_LIB_DIR` |
| `cannot find -lmlx` | Library path wrong | Verify: `ls -la /opt/homebrew/lib/libmlx*` |
| `Header not found: mlx/mlx.h` | Include path wrong | Check `MLX_INCLUDE_DIR` |
| Stub build when real expected | Headers missing or `MLX_FORCE_STUB=1` | Set environment variables, rebuild |

**Resolution:**
```bash
# Verify MLX installation
brew list mlx
brew --cellar mlx

# Explicitly set paths
export MLX_INCLUDE_DIR=/opt/homebrew/include
export MLX_LIB_DIR=/opt/homebrew/lib
cargo clean
cargo build -p adapteros-lora-mlx-ffi --features real-mlx --release
```

### Runtime Issues

| Problem | Cause | Solution |
|---------|-------|----------|
| "Model loads but forward fails" | Model path wrong or corrupted | Verify files exist: `ls -la model_dir/{config.json,model.safetensors,tokenizer.json}` |
| "Tokenizer not available" | tokenizer.json missing | Ensure it's in model directory |
| Circuit breaker opens | 3+ consecutive failures | Check `model.health_status()`, review logs |
| High memory usage | Leaks or large batch | Call `memory::gc_collect()`, reduce batch size |
| Slow inference | CPU fallback | Verify GPU: check MLX build logs |

**Debug Mode:**
```bash
# Enable debug logging
export RUST_LOG=debug,adapteros_lora_mlx_ffi=trace

# Run with backtrace
export RUST_BACKTRACE=full

# Check model loading
./target/release/aosctl load-model \
  --path /data/models/base/qwen2.5-7b-mlx \
  --verbose
```

### Performance Issues

**Slow Forward Passes**
```bash
# Profile inference
RUST_LOG=trace ./target/release/aosctl infer \
  --model ./models/qwen2.5-7b-mlx \
  --prompt "test" \
  --max-tokens 10

# Check GPU utilization
# Use Activity Monitor or:
# ps aux | grep aosctl  # Check CPU %
```

**High Memory Usage**
```bash
# Monitor memory
watch -n 1 'ps aux | grep aosctl | grep -v grep | awk "{print \$6}"'

# Reduce batch size in config
# Or trigger GC more aggressively
```

**Adapter Loading Failures**
```bash
# Verify adapter format
ls -la adapter.safetensors

# Check adapter compatibility with model
# Model hidden_size must match adapter dimensions
```

---

## Performance Tuning

### Memory Configuration

```toml
[mlx]
# Optimize for model size
max_memory_mb = 16000  # Allocate 16GB max
min_free_memory_mb = 1000  # Keep 1GB free
gc_threshold_mb = 2000  # Trigger GC at 2GB usage

# For larger models (13B+)
max_memory_mb = 32000
min_free_memory_mb = 2000
gc_threshold_mb = 4000

# For smaller models or CPU constraints
max_memory_mb = 8000
min_free_memory_mb = 500
gc_threshold_mb = 1000
```

### Batch Processing Optimization

```rust
// Optimal batch size depends on model and memory
// For 7B model: 4-8 concurrent requests
// For 13B model: 2-4 concurrent requests
// For 70B model: 1-2 concurrent requests

// Monitor and adjust based on metrics
let batch_size = if memory::exceeds_threshold(14000.0) {
    4  // Reduce batch
} else {
    8  // Full batch
};
```

### Inference Pipeline Optimization

```rust
use adapteros_lora_mlx_ffi::{MLXFFIModel, generation::GenerationConfig};

let config = GenerationConfig {
    max_tokens: 256,
    temperature: 0.7,
    top_k: Some(40),  // Reduce top-k for speed
    top_p: Some(0.9),
    repetition_penalty: 1.1,
    eos_token: 2,
    use_cache: true,  // Enable KV cache
};

// Use exact config for consistent latency
let text = model.generate_with_config(prompt, config)?;
```

### Adapter Hot-Swap Performance

```bash
# Pre-load frequently-used adapters during startup
# Reduces swap latency during runtime

# Monitor swap duration in logs:
# "Adapter preload completed in Xms"
```

---

## See Also

- [MLX_INTEGRATION.md](./MLX_INTEGRATION.md) - Complete integration reference
- [docs/ADR_MULTI_BACKEND_STRATEGY.md](./ADR_MULTI_BACKEND_STRATEGY.md) - Multi-backend architecture
- [docs/DETERMINISTIC_EXECUTION.md](./DETERMINISTIC_EXECUTION.md) - HKDF seeding details
- [docs/COREML_INTEGRATION.md](./COREML_INTEGRATION.md) - CoreML alternative backend

---

**Maintained by:** James KC Auchterlonie
**Last Updated:** 2025-11-22
**Status:** Production-Ready
