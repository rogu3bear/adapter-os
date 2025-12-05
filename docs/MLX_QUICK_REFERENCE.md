# MLX Backend Quick Reference

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2025-11-22

---

## Quick Start (5 Minutes)

### 1. Install MLX
```bash
brew install mlx
# Verify: ls /opt/homebrew/lib/libmlx*
```

### 2. Build with MLX
```bash
cargo build -p adapteros-lora-mlx-ffi --features mlx --release
# Check output for: "MLX FFI build: REAL"
```

### 3. Prepare Model
```bash
# Convert Hugging Face model to MLX format
pip install mlx-lm
python -m mlx_lm.convert \
  --hf-path Qwen/Qwen2.5-7B-Instruct \
  --mlx-path models/qwen2.5-7b-mlx
```

### 4. Start Server
```bash
export AOS_MLX_FFI_MODEL="./models/qwen2.5-7b-mlx"
./target/release/aosctl serve \
  --backend mlx \
  --model-path ./models/qwen2.5-7b-mlx
```

### 5. Test Inference
```bash
curl -X POST http://localhost:8080/v1/infer \
  -H "Content-Type: application/json" \
  -d '{"prompt": "Hello", "max_tokens": 10}'
```

---

## Common Configuration Patterns

### Development Setup
```toml
[mlx]
enabled = true
model_path = "./models/qwen2.5-7b-mlx"
default_backend = "mlx"

[mlx.resilience]
max_consecutive_failures = 3
enable_stub_fallback = true

[mlx.performance]
batch_size = 4
enable_kv_cache = true
```

### Production Setup
```toml
[mlx]
enabled = true
model_path = "/data/models/qwen2.5-7b-mlx"
default_backend = "mlx"
max_memory_mb = 20000
min_free_memory_mb = 2000
gc_threshold_mb = 3000

[mlx.resilience]
max_consecutive_failures = 5
circuit_breaker_timeout_secs = 300
enable_stub_fallback = false
health_check_interval_secs = 60

[mlx.performance]
batch_size = 8
prefetch_adapters = true
enable_kv_cache = true
cache_warmup_tokens = 512

[mlx.determinism]
use_hkdf_seeding = true
base_seed = "automatic"
```

### High-Performance Setup (GPU-intensive)
```toml
[mlx]
enabled = true
model_path = "/data/models/qwen2.5-7b-mlx"
default_backend = "mlx"
max_memory_mb = 28000  # Leave headroom
min_free_memory_mb = 3000
gc_threshold_mb = 5000

[mlx.performance]
batch_size = 16
prefetch_adapters = true
enable_kv_cache = true
cache_warmup_tokens = 1024

[mlx.resilience]
max_consecutive_failures = 5
circuit_breaker_timeout_secs = 600
```

---

## Code Examples

### Load & Inference
```rust
use adapteros_lora_mlx_ffi::MLXFFIModel;
use adapteros_core::Result;

fn main() -> Result<()> {
    // Load model
    let model = MLXFFIModel::load("./models/qwen2.5-7b-mlx")?;

    // Run forward pass
    let tokens = vec![1, 2, 3];
    let logits = model.forward(&tokens, 0)?;

    println!("Output shape: {} logits", logits.len());
    Ok(())
}
```

### Text Generation with Determinism
```rust
use adapteros_lora_mlx_ffi::MLXFFIModel;
use adapteros_core::{derive_seed, B3Hash};

fn main() -> Result<()> {
    let model = MLXFFIModel::load("./models/qwen2.5-7b-mlx")?;

    // Deterministic seeding
    let base_seed = B3Hash::hash(b"production-model");
    let seed = derive_seed(&base_seed, "text-generation:step-0");
    adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes(&seed)?;

    // Generate with reproducible results
    let text = model.generate("Once upon a time", 100)?;
    println!("{}", text);
    Ok(())
}
```

### Custom Generation Config
```rust
use adapteros_lora_mlx_ffi::{MLXFFIModel, generation::GenerationConfig};

fn main() -> Result<()> {
    let model = MLXFFIModel::load("./models/qwen2.5-7b-mlx")?;

    let config = GenerationConfig {
        max_tokens: 256,
        temperature: 0.7,       // 0.0 = deterministic, 1.0+ = random
        top_k: Some(50),        // Keep top 50 tokens
        top_p: Some(0.9),       // Nucleus sampling
        repetition_penalty: 1.1,
        eos_token: 2,
        use_cache: true,
    };

    let text = model.generate_with_config("Explain MLX", config)?;
    println!("{}", text);
    Ok(())
}
```

### Multi-Adapter Routing
```rust
use adapteros_lora_mlx_ffi::{MLXFFIBackend, MLXFFIModel};
use adapteros_lora_kernel_api::FusedKernels;

fn main() -> Result<()> {
    let model = MLXFFIModel::load("./models/qwen2.5-7b-mlx")?;
    let mut backend = MLXFFIBackend::new(model);

    // Load adapters
    backend.register_adapter(0, adapter1)?;
    backend.register_adapter(1, adapter2)?;
    backend.register_adapter(2, adapter3)?;

    // Route inference through selected adapters
    // (Details depend on router implementation)
    Ok(())
}
```

### Memory Monitoring
```rust
use adapteros_lora_mlx_ffi::memory;

fn main() {
    // Get current stats
    let stats = memory::stats();
    println!("{}", memory::format_stats(&stats));

    // Check threshold
    if memory::exceeds_threshold(15000.0) {  // 15GB
        println!("Memory high, triggering GC");
        memory::gc_collect();
    }

    // Reset for testing
    memory::reset();
}
```

### Health Monitoring
```rust
use adapteros_lora_mlx_ffi::MLXFFIModel;

fn main() -> Result<()> {
    let model = MLXFFIModel::load("./models/qwen2.5-7b-mlx")?;

    // Check health
    if let Some(health) = model.health_status() {
        println!("Operational: {}", health.operational);
        println!("Failures: {}", health.consecutive_failures);
        println!("Circuit breaker: {:?}", health.circuit_breaker);

        if !model.is_healthy() {
            println!("Resetting circuit breaker");
            model.reset_circuit_breaker();
        }
    }
    Ok(())
}
```

---

## Deployment Snippets

### Systemd Service
```ini
[Service]
Type=simple
User=aos
ExecStart=/opt/aos/bin/aosctl serve --backend mlx
Restart=on-failure
RestartSec=10s
Environment="AOS_MLX_FFI_MODEL=/data/models/qwen2.5-7b-mlx"
Environment="RUST_LOG=info,adapteros_lora_mlx_ffi=info"
```

### Docker Container
```dockerfile
FROM rust:1.75 as builder
RUN apt-get update && apt-get install -y mlx-dev
COPY . /app
WORKDIR /app
RUN cargo build -p adapteros-lora-mlx-ffi --features mlx --release

FROM debian:bookworm
RUN apt-get update && apt-get install -y mlx-runtime
COPY --from=builder /app/target/release/aosctl /usr/local/bin/
CMD ["aosctl", "serve", "--backend", "mlx"]
```

### Kubernetes Deployment
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: aos-mlx
spec:
  replicas: 1
  selector:
    matchLabels:
      app: aos-mlx
  template:
    metadata:
      labels:
        app: aos-mlx
    spec:
      nodeSelector:
        hardware.apple.com/silicon: "true"  # Apple Silicon
      containers:
      - name: mlx
        image: aos:latest
        command: ["aosctl", "serve", "--backend", "mlx"]
        env:
        - name: AOS_MLX_FFI_MODEL
          value: /data/models/qwen2.5-7b-mlx
        - name: RUST_LOG
          value: "info,adapteros_lora_mlx_ffi=info"
        resources:
          limits:
            memory: "24Gi"
            cpu: "8"
          requests:
            memory: "16Gi"
            cpu: "4"
        volumeMounts:
        - name: models
          mountPath: /data/models
          readOnly: true
      volumes:
      - name: models
        hostPath:
          path: /data/models
          type: Directory
```

---

## Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `AOS_MLX_FFI_MODEL` | Model path override | `/data/models/qwen2.5-7b-mlx` |
| `MLX_INCLUDE_DIR` | MLX headers path | `/opt/homebrew/include` |
| `MLX_LIB_DIR` | MLX library path | `/opt/homebrew/lib` |
| `RUST_LOG` | Log level | `info,adapteros_lora_mlx_ffi=debug` |
| `RUST_BACKTRACE` | Backtrace verbosity | `1` or `full` |
| `AOS_MLX_MAX_MEMORY_MB` | Memory limit | `16000` |
| `AOS_MLX_GC_THRESHOLD_MB` | GC trigger | `2000` |
| `MLX_FORCE_STUB` | Force stub build | `1` |

---

## Troubleshooting Checklist

### Build Issues
```bash
# Check MLX installation
brew list mlx
brew info mlx

# Verify paths
echo $MLX_INCLUDE_DIR
echo $MLX_LIB_DIR
ls -la /opt/homebrew/lib/libmlx*

# Rebuild
cargo clean
cargo build -p adapteros-lora-mlx-ffi --features mlx --release
```

### Runtime Issues
```bash
# Check logs
RUST_LOG=debug ./target/release/aosctl serve --backend mlx 2>&1 | tail -50

# Verify model files
ls -la ./models/qwen2.5-7b-mlx/
# Must have: config.json, model.safetensors, tokenizer.json

# Test model directly
./target/release/aosctl load-model --path ./models/qwen2.5-7b-mlx --verbose

# Check memory
ps aux | grep aosctl
memory stat
```

### Performance Issues
```bash
# Monitor during inference
watch -n 0.1 'ps aux | grep aosctl'

# Profile with logging
RUST_LOG=trace ./target/release/aosctl serve --backend mlx

# Check adapter compatibility
# Ensure adapter lora_A/lora_B dimensions match model hidden_size
```

---

## Common Tasks

### Switch Backend at Runtime
```rust
// Use factory to select backend
use adapteros_lora_worker::backend_factory::{BackendChoice, create_backend};

let backend = if should_use_mlx {
    create_backend(BackendChoice::Mlx {
        model_path: "/data/models/qwen2.5-7b-mlx".to_string(),
    })?
} else {
    create_backend(BackendChoice::Metal)?
};
```

### Implement Custom Sampling
```rust
use adapteros_lora_mlx_ffi::{mlx_sample_token_safe, MLXFFITensor};

let logits = MLXFFITensor::from_data(&[...logits_data], vocab_size)?;

// Sample with custom parameters
let token = mlx_sample_token_safe(
    &logits,
    temperature,  // Adjust randomness
    top_k,        // Filter to top K
    top_p,        // Nucleus sampling
)?;
```

### Monitor Model Health
```bash
# Query health endpoint
curl http://localhost:8080/healthz/backend | jq '.mlx_backend'

# Expected response:
# {
#   "operational": true,
#   "circuit_breaker": "Closed",
#   "consecutive_failures": 0,
#   "adapter_count": 3
# }
```

---

## Performance Benchmarks

### Inference Latency (7B Model, M2 Max)

| Operation | Latency | Notes |
|-----------|---------|-------|
| Model load | 500ms | One-time |
| Forward pass (1 token) | 15ms | Cold cache |
| Forward pass (batched) | 30ms | Batch size 4 |
| Text generation (100 tokens) | 2000ms | With sampling |
| Adapter hot-swap | 50ms | Runtime load |

### Memory Usage (7B Model)

| Component | Memory | Notes |
|-----------|--------|-------|
| Model weights | 4.5GB | INT8 quantized |
| KV cache | 1.2GB | Max sequence length |
| Adapters (5x) | 0.5GB | ~100MB each |
| Runtime overhead | 0.3GB | FFI, allocation tracking |
| **Total** | **~6.5GB** | Typical deployment |

---

## Getting Help

### Documentation
- [MLX_INTEGRATION.md](./MLX_INTEGRATION.md) - Complete integration guide
- [MLX_BACKEND_DEPLOYMENT_GUIDE.md](./MLX_BACKEND_DEPLOYMENT_GUIDE.md) - Detailed deployment steps
- [docs/COREML_INTEGRATION.md](./COREML_INTEGRATION.md) - Compare with CoreML backend

### Community
- GitHub Issues: Report bugs and feature requests
- Discussion Forum: Ask questions and share experiences
- Matrix Chat: Real-time community discussion

### Support Escalation
1. Check documentation and existing issues
2. Enable debug logging: `RUST_LOG=debug`
3. Capture full error output and logs
4. Submit GitHub issue with reproduction steps

---

**Quick Links:**
- [Full Integration Guide](./MLX_INTEGRATION.md)
- [Deployment Guide](./MLX_BACKEND_DEPLOYMENT_GUIDE.md)
- [Architecture Guide](./ADR_MULTI_BACKEND_STRATEGY.md)
- [CLAUDE.md](../CLAUDE.md) - Development standards

**Last Updated:** 2025-11-22
**Maintained by:** James KC Auchterlonie
