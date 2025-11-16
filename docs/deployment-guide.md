# AdapterOS Deployment Guide

This guide covers deploying AdapterOS on macOS in air-gapped environments, including installation, plan building, tenant setup, and inference operations.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Installation Methods](#installation-methods)
- [Air-Gapped Installation](#air-gapped-installation)
- [Plan Building](#plan-building)
- [Tenant Setup](#tenant-setup)
- [Inference Operations](#inference-operations)
- [Production Configuration](#production-configuration)
- [Troubleshooting](#troubleshooting)

## Prerequisites

### Hardware Requirements

- **Apple Silicon Mac** (M1/M2/M3/M4) - Intel Macs are not supported
- **Minimum 16GB RAM** - Recommended 32GB+ for production workloads
- **10GB+ free disk space** - For models, artifacts, and logs
- **macOS 13.0+** (Ventura or later)

### Software Dependencies

- **Rust 1.75+**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Xcode Command Line Tools**: `xcode-select --install`
- **Git**: For source code management

## Installation Methods

### Option 1: Graphical Installer (Recommended)

The native macOS installer provides guided setup with hardware validation:

```bash
# Build the installer
make installer

# Or open in Xcode for development
make installer-open
```

**Features:**
- Hardware pre-checks (Apple Silicon, RAM, disk space)
- Installation modes: Full (with model download) or Minimal (binaries only)
- Air-gapped support for offline installations
- Checkpoint recovery for interrupted installations
- Determinism education post-install

### Option 2: Manual Installation

For air-gapped or custom deployments:

```bash
# Clone the repository
git clone https://github.com/rogu3bear/adapter-os.git
cd adapter-os

# Build the workspace
cargo build --release

# Initialize the database
./target/release/aosctl init-tenant --id default --uid 1000 --gid 1000
```

## Air-Gapped Installation

For environments without internet access:

### Step 1: Prepare Installation Media

On a connected machine:

```bash
# Build the complete system
make build

# Create installation bundle
tar -czf adapteros-airgap.tar.gz \
  target/release/aosctl \
  target/release/aos-cp \
  configs/ \
  metal/ \
  migrations/ \
  manifests/ \
  scripts/bootstrap_with_checkpoints.sh
```

### Step 2: Transfer and Install

On the air-gapped machine:

```bash
# Extract the bundle
tar -xzf adapteros-airgap.tar.gz

# Run air-gapped bootstrap
bash scripts/bootstrap_with_checkpoints.sh \
  /tmp/adapteros_install.state \
  minimal \
  true \
  false
```

### Step 3: Verify Installation

```bash
# Check binaries
./target/release/aosctl --version
./target/release/aos-cp --version

# Verify Metal kernels compiled
ls metal/*.metallib

# Check database initialization
ls var/aos-cp.sqlite3
```

## Plan Building

Plans define the execution configuration for inference. Build plans from manifests:

### Step 1: Create Manifest

Create a manifest file (YAML or JSON):

```yaml
# manifests/my-plan.yaml
schema: adapteros.manifest.v3
base:
  model_id: "Qwen2.5-7B-Instruct"
  model_hash: "b3:9089587768b6a4fd"
  arch: "Qwen2ForCausalLM"
  vocab_size: 152064
  hidden_dim: 3584
  n_layers: 28
  n_heads: 28
router:
  k_sparse: 3
  gate_quant: "q15"
  entropy_floor: 0.02
  tau: 1.0
  sample_tokens_full: 128
telemetry:
  schema_hash: "b3:stub"
  sampling:
    token: 0.05
    router: 1.0
    inference: 1.0
  router_full_tokens: 128
  bundle:
    max_events: 500000
    max_bytes: 268435456
policies:
  egress: "deny_all"
  access:
    adapters: "RBAC"
    datasets: "ABAC"
seeds:
  global: "b3:deadbeef"
```

### Step 2: Build Plan

```bash
# Build plan from manifest
./target/release/aosctl build-plan \
  --manifest manifests/my-plan.yaml \
  --output plan/my-plan \
  --tenant-id default
```

### Step 3: Verify Plan

```bash
# List available plans
./target/release/aosctl list-plans --tenant-id default

# Inspect plan details
./target/release/aosctl plan-info --plan-id my-plan --tenant-id default
```

## Tenant Setup

Tenants provide isolation and resource management:

### Step 1: Initialize Tenant

```bash
# Create a new tenant
./target/release/aosctl init-tenant \
  --id production \
  --uid 5000 \
  --gid 5000

# Verify tenant creation
./target/release/aosctl list-tenants
```

### Step 2: Configure Tenant Directories

```bash
# Create tenant-specific directories
sudo mkdir -p /var/run/aos/production
sudo chown 5000:5000 /var/run/aos/production
sudo chmod 755 /var/run/aos/production
```

### Step 3: Set Up Model (Air-Gapped)

For air-gapped environments, models must be pre-loaded:

```bash
# Copy model files to the system
cp -r /path/to/qwen2.5-7b-mlx/ models/

# Verify model structure
ls models/qwen2.5-7b-mlx/
# Should contain: weights.safetensors, config.json, tokenizer.json, etc.
```

## Inference Operations

### Step 1: Start Control Plane

```bash
# Start the control plane server
./target/release/aos-cp --config configs/cp.toml
```

The control plane provides:
- REST API on port 8080
- Database management
- Worker coordination
- Policy enforcement

### Step 2: Start Worker

In a separate terminal:

```bash
# Start inference worker
./target/release/aosctl serve \
  --tenant production \
  --plan my-plan \
  --socket /var/run/aos/production/inference.sock \
  --backend metal
```

### Step 3: Run Inference

```bash
# Test inference via CLI
./target/release/aosctl infer \
  --prompt "Explain how AdapterOS works" \
  --max-tokens 100 \
  --tenant production

# Or via HTTP API
curl -X POST http://localhost:8080/v1/inference \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_JWT_TOKEN" \
  -d '{
    "prompt": "Explain how AdapterOS works",
    "max_tokens": 100,
    "tenant_id": "production"
  }'
```

## Production Configuration

### Security Configuration

Update `configs/cp.toml` for production:

```toml
[security]
# MUST be true in production!
require_pf_deny = true
mtls_required = true
jwt_secret = "GENERATE_64_CHAR_RANDOM_STRING_HERE"

[server]
port = 8080
bind = "127.0.0.1"  # Or specific interface

[worker.safety]
inference_timeout_secs = 30
evidence_timeout_secs = 5
max_concurrent_requests = 10
max_tokens_per_second = 40
```

### Packet Filter (PF) Rules

For air-gapped operation, configure PF to block all outbound traffic:

```bash
# Create PF configuration
sudo tee /etc/pf.conf << 'EOF'
# Block all outbound traffic
block out all

# Allow loopback
pass on lo0

# Allow inbound on specific ports
pass in proto tcp from any to any port 8080
EOF

# Enable PF
sudo pfctl -e -f /etc/pf.conf

# Verify PF is active
sudo pfctl -s info
```

### Process Management

For production deployments, use a process manager:

```bash
# Create systemd service (if using systemd)
sudo tee /etc/systemd/system/adapteros-cp.service << 'EOF'
[Unit]
Description=AdapterOS Control Plane
After=network.target

[Service]
Type=simple
User=aos
Group=aos
WorkingDirectory=/opt/adapteros
ExecStart=/opt/adapteros/target/release/aos-cp --config configs/cp.toml
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl enable adapteros-cp
sudo systemctl start adapteros-cp
```

## Troubleshooting

### Common Issues

**1. Metal Kernel Compilation Failures**
```bash
# Rebuild Metal kernels
cd metal && bash build.sh

# Check for Xcode command line tools
xcode-select --install
```

**2. Database Connection Issues**
```bash
# Check database file permissions
ls -la var/aos-cp.sqlite3

# Reinitialize database
rm var/aos-cp.sqlite3
./target/release/aos-cp --config configs/cp.toml --migrate-only
```

**3. Worker Startup Failures**
```bash
# Check socket permissions
ls -la /var/run/aos/production/

# Verify tenant exists
./target/release/aosctl list-tenants

# Check worker logs
tail -f var/logs/worker.log
```

**4. Inference Timeouts**
```bash
# Check worker health
./target/release/aosctl worker-status --tenant production

# Verify model loading
./target/release/aosctl model-info --tenant production

# Check memory usage
./target/release/aosctl system-metrics
```

### Logging and Debugging

Enable debug logging:

```bash
# Set environment variables
export RUST_LOG=debug
export RUST_BACKTRACE=1

# Run with verbose output
./target/release/aosctl serve --tenant production --plan my-plan --verbose
```

### Performance Tuning

For production workloads:

1. **Memory Management**: Ensure ≥15% unified memory headroom
2. **Router Configuration**: Adjust K-sparse and entropy floor
3. **Batch Processing**: Use multiple workers for high throughput
4. **Model Optimization**: Use quantized models for faster inference

### Health Checks

Monitor system health:

```bash
# Check control plane health
curl http://localhost:8080/healthz

# Check worker status
./target/release/aosctl worker-status --tenant production

# Monitor system metrics
./target/release/aosctl system-metrics --interval 30
```

## Next Steps

After successful deployment:

1. **Register Adapters**: Add domain-specific LoRA adapters
2. **Configure Policies**: Set up the 20 policy packs
3. **Set Up Monitoring**: Configure telemetry and alerting
4. **Load Test**: Validate performance under load
5. **Backup Strategy**: Implement artifact and database backups

For advanced configuration and customization, see:
- [Architecture Documentation](architecture.md)
- [Policy Configuration](POLICIES.md)
- [API Reference](control-plane.md)
