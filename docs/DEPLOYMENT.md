# adapterOS Deployment Guide

**Complete deployment guide for adapterOS, covering single-node production setup, air-gapped deployment, authentication, and verification.**

---

## Table of Contents

1. [Deployment Overview](#deployment-overview)
2. [Prerequisites](#prerequisites)
3. [Single-Node Deployment](#single-node-deployment)
4. [Configuration](#configuration)
5. [Verification Steps](#verification-steps)
6. [Air-Gapped Deployment](#air-gapped-deployment)
7. [Multi-Node Cluster Setup](#multi-node-cluster-setup)
8. [Kubernetes Deployment](#kubernetes-deployment)
9. [Authentication Configuration](#authentication-configuration)
10. [Scaling Guidelines](#scaling-guidelines)
11. [Production Checklist](#production-checklist)
12. [Troubleshooting](#troubleshooting)

---

## Deployment Overview

adapterOS is an ML inference platform with an offline-capable, UMA-optimized orchestration layer for multi-LoRA systems on Apple Silicon.

### Core Technologies

- **DIR (Deterministic Inference Runtime)**: The core execution engine ensuring reproducible, auditable inference with token-level determinism
- **TAS (Token Artifact System)**: Transforms inference outputs into persistent, reusable artifacts for composition and audit trails

### Architecture Diagram

```mermaid
graph TB
    subgraph Client["Client Layer"]
        HTTP[HTTP/REST API]
        CLI[aosctl CLI]
        UI[Web UI :3200]
    end

    subgraph ControlPlane["Control Plane :8080"]
        API[API Server<br/>adapteros-server]
        Auth[JWT Auth]
        Policy[Policy Engine<br/>25 Policy Packs]
        DB[(SQLite DB<br/>var/aos-cp.sqlite3)]
    end

    subgraph Worker["Worker Layer"]
        UDS[Unix Domain Socket]
        Router[K-Sparse Router<br/>Q15 Quantization]
        Backend[Multi-Backend<br/>CoreML/Metal/MLX]
        Memory[UMA Memory<br/>Adapter Cache]
    end

    subgraph Storage["Storage Layer"]
        Adapters[Adapter Registry<br/>.aos packages]
        Models[Base Models<br/>safetensors]
        Telemetry[Telemetry Bundles<br/>JSONL]
    end

    HTTP --> API
    CLI --> API
    UI --> API

    API --> Auth
    Auth --> Policy
    Policy --> DB

    API -.UDS.-> UDS
    UDS --> Router
    Router --> Backend
    Backend --> Memory

    Memory --> Adapters
    Backend --> Models
    Policy --> Telemetry

    style ControlPlane fill:#e1f5ff,stroke:#333
    style Worker fill:#fff4e1,stroke:#333
    style Storage fill:#e8f8e8,stroke:#333
```

### Deployment Flow

```mermaid
flowchart TD
    Start([Start Deployment]) --> PreCheck{Prerequisites<br/>Met?}

    PreCheck -->|No| InstallDeps[Install Dependencies<br/>Rust, Xcode CLI Tools]
    InstallDeps --> PreCheck

    PreCheck -->|Yes| ChooseMethod{Installation<br/>Method}

    ChooseMethod -->|Graphical| Installer[GUI Installer<br/>xcodebuild adapterOSInstaller]
    ChooseMethod -->|Manual| Build[Build from Source<br/>cargo build --release]
    ChooseMethod -->|Air-Gapped| Bundle[Transfer Bundle<br/>adapteros-airgap.tar.gz]

    Installer --> InitDB[Initialize Database<br/>aosctl db migrate]
    Build --> InitDB
    Bundle --> InitDB

    InitDB --> Tenant[Create Tenant<br/>aosctl init-tenant]
    Tenant --> LoadModel[Load Base Model<br/>Qwen2.5-7B]
    LoadModel --> Config[Configure Production<br/>configs/production.toml]

    Config --> Security{Security<br/>Mode}
    Security -->|Production| SecConfig[Enable JWT + PF Rules]
    Security -->|Development| DevConfig[Development Auth]

    SecConfig --> StartCP[Start Control Plane<br/>adapteros-server]
    DevConfig --> StartCP

    StartCP --> StartWorker[Start Worker<br/>aos-worker via UDS]
    StartWorker --> Verify[Verification]

    Verify --> HealthCheck{Health Check<br/>Pass?}
    HealthCheck -->|No| Debug[Check Logs]
    Debug --> Verify

    HealthCheck -->|Yes| Inference{Test Inference<br/>Success?}
    Inference -->|No| Debug

    Inference -->|Yes| Monitor[Enable Monitoring<br/>Prometheus + Grafana]
    Monitor --> Complete([Deployment Complete])

    style Start fill:#e1f5ff,stroke:#333
    style Complete fill:#e8f8e8,stroke:#333
    style Debug fill:#ffe1e1,stroke:#333
```

**Key Characteristics:**
- **Single-Node**: Optimized for standalone deployment on Apple Silicon
- **Multi-Tenant**: Isolated workspaces with RBAC
- **Zero Egress**: No network traffic during inference (UDS-only)
- **Deterministic**: Reproducible results with seed derivation
- **Hot-Swap**: Live adapter loading without restart

---

## Prerequisites

### Hardware Requirements

- **Apple Silicon Mac** (M1/M2/M3/M4) - **Intel Macs NOT supported**
- **Minimum RAM**: 16GB (32GB+ recommended for production)
- **Disk Space**: 10GB minimum (100GB+ for production with multiple models)
- **macOS Version**: 13.0+ (Ventura or later)
- **Network**: Gigabit Ethernet for multi-node (optional)

### Software Requirements

**Required:**
- **Rust 1.75+**: Install via rustup
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **Xcode Command Line Tools**:
  ```bash
  xcode-select --install
  ```
- **Git**: For source code management

**Optional:**
- **PostgreSQL 15+** with pgvector (for multi-node RAG)
- **Docker** (for containerized deployments)
- **direnv** (for environment management)

### Pre-Installation Checks

```bash
# Verify Apple Silicon
uname -m  # Should output: arm64

# Check available RAM
sysctl hw.memsize  # Should be ≥17179869184 (16GB)

# Check disk space
df -h /  # Should have ≥10GB free

# Verify macOS version
sw_vers  # ProductVersion should be ≥13.0
```

---

## Single-Node Deployment

### Method 1: Graphical Installer (Recommended)

The native macOS installer provides guided setup with hardware validation:

```bash
# Build the installer
cd installer && xcodebuild -project adapterOSInstaller.xcodeproj -scheme adapterOSInstaller -configuration Release -derivedDataPath build clean build

# Or open in Xcode for development
open installer/adapterOSInstaller.xcodeproj
```

**Features:**
- Hardware pre-checks (Apple Silicon, RAM, disk space)
- Installation modes: Full (with model download) or Minimal (binaries only)
- Air-gapped support for offline installations
- Checkpoint recovery for interrupted installations
- Determinism education post-install

**Installation Steps:**
1. Launch installer app
2. Select installation mode (Full/Minimal)
3. Choose installation directory
4. Review hardware requirements
5. Complete installation
6. Follow post-install verification

### Method 2: Manual Installation

For custom deployments or development:

```bash
# Clone the repository
git clone https://github.com/rogu3bear/adapter-os.git
cd adapter-os

# Build the workspace (release mode)
cargo build --release

# Build Metal shaders
cd metal && bash build.sh

# Initialize database and create default tenant
./aosctl db migrate
./aosctl init-tenant --id default --uid 1000 --gid 1000
```

### Method 3: Quick Start Script

For rapid development setup:

```bash
# Use the canonical boot script
./start

# This runs:
# - Backend server on port 8080
# - UI dev server on port 3200
# - Auto-configures routes and services
```

### Database Setup

**Option 1: SQLite (Default - Single Node)**

```bash
# Database is automatically created at:
# var/aos-cp.sqlite3

# Verify database
ls -lh var/aos-cp.sqlite3

# Run migrations
./aosctl db migrate
```

**Option 2: PostgreSQL (Multi-Node)**

```bash
# Install PostgreSQL with pgvector
brew install postgresql@15 pgvector

# Start PostgreSQL
brew services start postgresql@15

# Create database
createdb adapteros_prod

# Enable pgvector extension
psql adapteros_prod -c "CREATE EXTENSION vector;"

# Set environment variable
export DATABASE_URL="postgresql://localhost/adapteros_prod"
export RAG_EMBED_DIM=3584
```

### Build adapterOS

```bash
# Single-node build (SQLite)
cargo build --release

# Multi-node build (PostgreSQL + RAG)
cargo build --release --features rag-pgvector

# Install binaries (optional)
sudo cp target/release/aosctl /usr/local/bin/
sudo cp target/release/adapteros-server /usr/local/bin/
sudo cp target/release/aos-worker /usr/local/bin/
```

### Initialize System

```bash
# Run database migrations
./aosctl db migrate

# Create production tenant
./aosctl init-tenant \
  --id production \
  --uid 5000 \
  --gid 5000

# Verify tenant creation
./aosctl list-tenants
```

### Load Base Model

```bash
# Import base model (example: Qwen2.5-7B)
./aosctl import-model \
  --name qwen2.5-7b-instruct \
  --weights models/qwen2.5-7b-mlx/weights.safetensors \
  --config models/qwen2.5-7b-mlx/config.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json

# Verify model import
./aosctl list-models
```

---

## Configuration

### Production Configuration

Create `configs/production.toml`:

```toml
[server]
port = 8080
bind_address = "0.0.0.0"  # Or "127.0.0.1" for localhost only
workers = 4

[db]
# SQLite (single-node)
url = "sqlite:var/aos-cp.sqlite3"

# PostgreSQL (multi-node) - uncomment for cluster setup
# url = "postgresql://adapteros:password@localhost/adapteros_prod"
# pool_size = 20

[security]
require_pf_deny = true  # Enforce packet filter rules
jwt_secret_path = "/etc/adapteros/jwt.secret"
mtls_required = false  # Set true for mutual TLS

[paths]
plan_dir = "/var/lib/adapteros/plans"
artifact_dir = "/var/lib/adapteros/artifacts"
adapters_root = "/var/lib/adapteros/adapters"

[router]
k_sparse = 3              # Number of adapters to activate
entropy_floor = 0.02      # Minimum routing entropy
gate_quant = "q15"        # Q15 quantization (32767 denominator)

[memory]
min_headroom_pct = 15     # Reserve 15% UMA memory
evict_order = ["ephemeral_ttl", "cold_lru", "warm_lru"]

[telemetry]
enabled = true
json_output = "/var/log/adapteros/telemetry.jsonl"
prometheus_port = 9090

[policies]
# Enable all 25 policy packs
egress = true
determinism = true
router = true
evidence = true
refusal = true
numeric = true
rag = true
isolation = true
telemetry = true
retention = true
performance = true
memory = true
artifacts = true
secrets = true
build_release = true
compliance = true
incident = true
output = true
adapters = true
deterministic_io = true
drift = true
```

### Development Configuration

Create `configs/cp-dev.toml`:

```toml
[server]
port = 8080
bind_address = "127.0.0.1"
workers = 2

[db]
url = "sqlite:var/aos-cp.sqlite3"

[auth]
mode = "development"
dev_token = "adapteros-local"
token_expiry_hours = 24

[security]
require_https = false
require_pf_deny = false  # Disabled for development
cors_origins = ["http://localhost:3200"]
enable_rate_limiting = false

[router]
k_sparse = 3
entropy_floor = 0.02
gate_quant = "q15"

[memory]
min_headroom_pct = 10

[telemetry]
enabled = true
json_output = "var/logs/telemetry.jsonl"
```

### Environment Variables

Create `.env` file:

```bash
# Database
DATABASE_URL=sqlite:var/aos-cp.sqlite3
# DATABASE_URL=postgresql://localhost/adapteros_prod  # For PostgreSQL

# Paths
AOS_ADAPTERS_ROOT=/var/lib/adapteros/adapters
AOS_PLAN_DIR=/var/lib/adapteros/plans

# RAG (if using pgvector)
RAG_EMBED_DIM=3584

# Development
RUST_LOG=info
RUST_BACKTRACE=1

# Authentication (development only)
AOS_DEV_NO_AUTH=1  # Bypass auth in debug builds
# AOS_DEV_JWT_SECRET=my-test-secret  # Or use custom secret
```

**Load environment with direnv:**

```bash
# Install direnv
brew install direnv

# Enable for current directory
direnv allow

# Variables auto-load from .env and .env.local
```

**Manual loading:**

```bash
set -a
source .env
source .env.local
set +a
```

### Plan Building

Plans define execution configuration for inference:

**Step 1: Create Manifest**

Create `manifests/production-plan.yaml`:

```yaml
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

**Step 2: Build Plan**

```bash
./aosctl build-plan \
  --manifest manifests/production-plan.yaml \
  --output plan/production-plan \
  --tenant-id production
```

**Step 3: Verify Plan**

```bash
# List plans
./aosctl list-plans --tenant-id production

# Inspect plan details
./aosctl plan-info \
  --plan-id production-plan \
  --tenant-id production
```

---

## Verification Steps

### Step 1: Start Services

```bash
# Start control plane
./target/release/adapteros-server --config configs/production.toml

# In separate terminal: Start worker
./target/release/aos-worker \
  --tenant production \
  --socket /var/run/aos/production/inference.sock \
  --backend mlx
```

### Step 2: Health Checks

```bash
# Control plane health
curl http://localhost:8080/healthz

# Expected output:
# {"status":"ok","version":"1.0.0"}

# Worker status
./aosctl worker-status --tenant production

# Expected output:
# Worker Status: READY
# Backend: mlx
# Adapters Loaded: 0
# Memory Usage: 5.2GB / 32GB (16% used)
```

### Step 3: Test Inference

**Via CLI:**

```bash
./aosctl infer \
  --prompt "Explain how adapterOS works in 50 words" \
  --max-tokens 100 \
  --tenant production
```

**Via HTTP API:**

```bash
# Get JWT token first (production mode)
TOKEN=$(curl -s -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@example.com","password":"password"}' \
  | jq -r '.token')

# Run inference
curl -X POST http://localhost:8080/v1/inference \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "prompt": "Explain how adapterOS works",
    "max_tokens": 100,
    "tenant_id": "production"
  }'
```

### Step 4: Verify Determinism

```bash
# Run determinism test suite
cargo test --test determinism_core_suite -- --test-threads=8
cargo test -p adapteros-lora-router --test determinism
bash scripts/check_fast_math_flags.sh

# Or manually test same prompt twice:
./aosctl infer \
  --prompt "Hello world" \
  --seed 42 \
  --temperature 0.0 \
  > output1.txt

./aosctl infer \
  --prompt "Hello world" \
  --seed 42 \
  --temperature 0.0 \
  > output2.txt

# Outputs should be identical
diff output1.txt output2.txt
```

### Step 5: Monitor System Metrics

```bash
# System metrics
./aosctl system-metrics

# Expected output:
# Memory: 5.2GB / 32GB (16% used, 27GB free)
# Adapters: 0 loaded
# Inference Rate: 0 req/s
# Avg Latency: N/A

# Check telemetry
tail -f /var/log/adapteros/telemetry.jsonl
```

### Step 6: Verify Policy Enforcement

```bash
# Check active policies
./aosctl list-policies

# Test egress policy (should fail)
./aosctl infer \
  --prompt "Make an HTTP request" \
  --adapter network-adapter

# Expected: Policy violation error
```

---

## Air-Gapped Deployment

For environments without internet access:

### Air-Gapped Installation Flow

```mermaid
flowchart TD
    subgraph Connected[Connected Machine]
        Build[1. Build System<br/>./scripts/fresh-build.sh + cargo build]
        Bundle[2. Create Bundle<br/>tar -czf adapteros-airgap.tar.gz]

        Build --> Bundle
    end

    subgraph Transfer[Transfer]
        Bundle --> Media[3. Copy to USB/Media]
        Media --> AirGap[4. Transfer to Air-Gapped Machine]
    end

    subgraph AirGapped[Air-Gapped Machine]
        Extract[5. Extract Bundle<br/>tar -xzf]
        Bootstrap[6. Run Bootstrap<br/>bootstrap_with_checkpoints.sh]
        Verify[7. Verify Installation]

        AirGap --> Extract
        Extract --> Bootstrap

        Bootstrap --> CheckBin{Binaries<br/>Present?}
        CheckBin -->|No| Fail1[❌ Installation Failed]
        CheckBin -->|Yes| CheckMetal{Metal Kernels<br/>Compiled?}

        CheckMetal -->|No| Fail2[❌ Metal Compilation Failed]
        CheckMetal -->|Yes| CheckDB{Database<br/>Initialized?}

        CheckDB -->|No| Fail3[❌ DB Initialization Failed]
        CheckDB -->|Yes| Success([✅ Installation Complete])

        Verify --> CheckBin
    end

    style Build fill:#e1f5ff,stroke:#333
    style Bootstrap fill:#fff4e1,stroke:#333
    style Success fill:#e8f8e8,stroke:#333
    style Fail1 fill:#ffe1e1,stroke:#333
    style Fail2 fill:#ffe1e1,stroke:#333
    style Fail3 fill:#ffe1e1,stroke:#333
```

### Step 1: Prepare Offline Bundle

On a machine with internet access:

```bash
# Clone repository with vendored dependencies
git clone --recursive https://github.com/rogu3bear/adapter-os.git
cd adapter-os

# Build the complete system
./scripts/fresh-build.sh
cargo build --release --locked --offline
./scripts/build_metadata.sh
./scripts/record_env.sh
./scripts/strip_timestamps.sh

# Vendor Rust dependencies
cargo vendor

# Build offline
cargo build --release --offline

# Create installation bundle
tar -czf adapteros-airgap.tar.gz \
  target/release/aosctl \
  target/release/adapteros-server \
  target/release/aos-worker \
  configs/ \
  metal/ \
  migrations/ \
  manifests/ \
  models/ \
  scripts/bootstrap_with_checkpoints.sh
```

**Installation Bundle Contents:**
- Compiled binaries (`aosctl`, `adapteros-server`, `aos-worker`)
- Precompiled Metal kernels (`.metallib`)
- Configuration files
- Database migrations
- Manifest templates
- Pre-downloaded models (optional)

### Step 2: Transfer and Install

On the air-gapped machine:

```bash
# Extract the bundle
tar -xzf adapteros-airgap.tar.gz

# Run air-gapped bootstrap
bash scripts/bootstrap_with_checkpoints.sh \
  var/installer/adapteros_install.state \
  minimal \
  true \
  false

# Install binaries
sudo cp target/release/* /usr/local/bin/
```

### Step 3: Configure Zero Egress

Edit `configs/airgap.toml`:

```toml
[security]
require_pf_deny = true
zero_network_egress = true

[egress]
# Block all network except Unix sockets
allowed_protocols = []
unix_socket_only = true
pf_rules_path = "/etc/pf.anchors/adapteros"
```

### Step 4: Packet Filter (PF) Rules

Create `/etc/pf.anchors/adapteros`:

```
# Block all outbound traffic from adapterOS process
block drop out proto tcp from any to any user adapteros
block drop out proto udp from any to any user adapteros
block drop out proto icmp from any to any user adapteros

# Allow loopback only
pass out proto tcp from 127.0.0.1 to 127.0.0.1 user adapteros
pass out on lo0 user adapteros
```

Enable PF rules:

```bash
# Load anchor
sudo pfctl -f /etc/pf.conf
sudo pfctl -a adapteros -f /etc/pf.anchors/adapteros
sudo pfctl -e

# Verify PF is active
sudo pfctl -s info

# Test with tcpdump (should show NO outbound traffic)
sudo tcpdump -i any -n src host <server_ip>
```

### Step 5: Verify Air-Gapped Operation

```bash
# Check binaries
./aosctl --version
./target/release/adapteros-server --version

# Verify Metal kernels compiled
ls metal/*.metallib

# Check database initialization
ls var/aos-cp.sqlite3

# Start services and verify no network traffic
./target/release/adapteros-server --config configs/airgap.toml &
sleep 5
sudo lsof -i -P | grep adapteros  # Should show NO network connections
```

---

## Multi-Node Cluster Setup

### Architecture

```
┌────────────────┐      ┌────────────────┐      ┌────────────────┐
│   Node 1       │      │   Node 2       │      │   Node 3       │
│  (Leader)      │◄────►│  (Worker)      │◄────►│  (Worker)      │
│                │      │                │      │                │
│  PostgreSQL    │      │  Inference     │      │  Inference     │
│  Control Plane │      │  Worker        │      │  Worker        │
└────────────────┘      └────────────────┘      └────────────────┘
         │                      │                       │
         └──────────────────────┴───────────────────────┘
                          Shared Storage
```

### 1. Shared PostgreSQL Setup

On **Node 1** (leader):

```bash
# Configure PostgreSQL for remote access
# Edit postgresql.conf
listen_addresses = '*'
max_connections = 100

# Edit pg_hba.conf (add nodes 2 and 3)
host    adapteros_prod    adapteros    192.168.1.0/24    md5

# Create database user
psql -c "CREATE USER adapteros WITH PASSWORD 'secure_password';"
psql -c "GRANT ALL PRIVILEGES ON DATABASE adapteros_prod TO adapteros;"

# Restart PostgreSQL
brew services restart postgresql@15

# Verify remote connection
psql -h 192.168.1.10 -U adapteros -d adapteros_prod
```

### 2. Worker Node Configuration

On **Node 2 and Node 3**:

```bash
# Set database URL to point to leader
export DATABASE_URL="postgresql://adapteros:secure_password@192.168.1.10/adapteros_prod"

# Set federation mode
export AOS_FEDERATION_MODE=cluster

# Set unique node ID
export AOS_NODE_ID=node2  # or node3

# Build and start worker
cargo build --release --features rag-pgvector
./target/release/adapteros-server --config configs/worker.toml
```

Worker config (`configs/worker.toml`):

```toml
[server]
port = 8081  # Different port per worker (8081, 8082, etc.)
bind_address = "0.0.0.0"
workers = 8

[db]
url = "postgresql://adapteros:secure_password@192.168.1.10/adapteros_prod"
pool_size = 20

[federation]
mode = "cluster"
leader_url = "http://192.168.1.10:8080"
heartbeat_interval_secs = 10

# ... rest same as production.toml
```

### 3. Leader Election

Leader election is automatic via PostgreSQL:

```sql
-- Check current leader
SELECT node_id, elected_at
FROM cluster_nodes
WHERE is_leader = TRUE;

-- View all cluster nodes
SELECT node_id, is_leader, last_heartbeat, status
FROM cluster_nodes
ORDER BY elected_at;
```

---

## Kubernetes Deployment

### 1. Create Namespace

```bash
kubectl create namespace adapteros
```

### 2. PostgreSQL StatefulSet

Create `k8s/postgres.yaml`:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: postgres
  namespace: adapteros
spec:
  ports:
  - port: 5432
  clusterIP: None
  selector:
    app: postgres
---
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: postgres
  namespace: adapteros
spec:
  serviceName: postgres
  replicas: 1
  selector:
    matchLabels:
      app: postgres
  template:
    metadata:
      labels:
        app: postgres
    spec:
      containers:
      - name: postgres
        image: pgvector/pgvector:pg15
        ports:
        - containerPort: 5432
        env:
        - name: POSTGRES_DB
          value: adapteros_prod
        - name: POSTGRES_USER
          value: adapteros
        - name: POSTGRES_PASSWORD
          valueFrom:
            secretKeyRef:
              name: postgres-secret
              key: password
        volumeMounts:
        - name: postgres-storage
          mountPath: /var/lib/postgresql/data
  volumeClaimTemplates:
  - metadata:
      name: postgres-storage
    spec:
      accessModes: [ "ReadWriteOnce" ]
      resources:
        requests:
          storage: 100Gi
```

### 3. adapterOS Deployment

Create `k8s/adapteros.yaml`:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: adapteros-api
  namespace: adapteros
spec:
  type: LoadBalancer
  ports:
  - port: 8080
    targetPort: 8080
  selector:
    app: adapteros
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: adapteros
  namespace: adapteros
spec:
  replicas: 3
  selector:
    matchLabels:
      app: adapteros
  template:
    metadata:
      labels:
        app: adapteros
    spec:
      containers:
      - name: adapteros
        image: adapteros:latest
        ports:
        - containerPort: 8080
        env:
        - name: DATABASE_URL
          value: "postgresql://adapteros:password@postgres:5432/adapteros_prod"
        - name: RAG_EMBED_DIM
          value: "3584"
        resources:
          requests:
            memory: "16Gi"
            cpu: "4"
          limits:
            memory: "32Gi"
            cpu: "8"
        volumeMounts:
        - name: config
          mountPath: /etc/adapteros
        - name: adapters
          mountPath: /var/lib/adapteros/adapters
      volumes:
      - name: config
        configMap:
          name: adapteros-config
      - name: adapters
        persistentVolumeClaim:
          claimName: adapters-pvc
```

### 4. Deploy

```bash
# Create secrets
kubectl create secret generic postgres-secret \
  --from-literal=password=secure_password \
  -n adapteros

# Create config map
kubectl create configmap adapteros-config \
  --from-file=production.toml \
  -n adapteros

# Deploy
kubectl apply -f k8s/postgres.yaml
kubectl apply -f k8s/adapteros.yaml

# Check status
kubectl get pods -n adapteros
kubectl logs -f deployment/adapteros -n adapteros
```

---

## Authentication Configuration

> **Note:** For detailed authentication setup, see [docs/AUTHENTICATION.md](AUTHENTICATION.md).

### Pre-Deployment Security Checklist

Before deploying to production, complete these authentication security checks:

- [ ] JWT mode configured (`EdDSA` recommended for production)
- [ ] Strong JWT secret generated or keypair configured
- [ ] Authentication mode set to `production`
- [ ] Development tokens disabled or removed
- [ ] HTTPS enabled and enforced (if exposed externally)
- [ ] CORS origins restricted to production domains
- [ ] Rate limiting enabled
- [ ] Token expiry configured appropriately (recommended: 8 hours)
- [ ] Security logging enabled
- [ ] Failed login monitoring configured

### Development Environment

**Configuration** (`configs/cp-dev.toml`):

```toml
[auth]
mode = "development"
dev_token = "adapteros-local"
token_expiry_hours = 24

[security]
require_https = false
cors_origins = ["http://localhost:3200"]
enable_rate_limiting = false
```

**Starting the Server**:

```bash
./target/release/adapteros-server \
  --skip-pf-check \
  --config configs/cp-dev.toml
```

**Development Auth Bypass** (Debug builds only):

```bash
# Option 1: No-auth bypass
AOS_DEV_NO_AUTH=1 cargo run --bin adapteros-server

# Option 2: Custom JWT secret
AOS_DEV_JWT_SECRET="my-test-secret" cargo run --bin adapteros-server
```

### Production Environment

**Configuration** (`configs/cp-production.toml`):

```toml
[auth]
mode = "production"  # Strict JWT only, NO dev_token
token_expiry_hours = 8
max_login_attempts = 5
lockout_duration_minutes = 30

[security]
require_https = true
cors_origins = [
  "https://app.adapteros.example.com"
]
enable_rate_limiting = true
```

### JWT Configuration Modes

#### HMAC Mode (Simple, Shared Secret)

**Use case:** Single-server deployments

```bash
# Generate secret
openssl rand -base64 32 > var/jwt_secret.key

# Set restrictive permissions
chmod 600 var/jwt_secret.key

# Set in config
[jwt]
mode = "hmac"
secret_file = "var/jwt_secret.key"
```

#### EdDSA Mode (Public Key, More Secure)

**Use case:** Production, distributed systems (recommended)

```bash
# Generate Ed25519 keypair
openssl genpkey -algorithm Ed25519 -out var/jwt_private.pem
openssl pkey -in var/jwt_private.pem -pubout -out var/jwt_public.pem

# Set restrictive permissions
chmod 600 var/jwt_private.pem
chmod 644 var/jwt_public.pem

# Set in config
[jwt]
mode = "eddsa"
private_key_file = "var/jwt_private.pem"
public_key_file = "var/jwt_public.pem"
```

### HTTPS Configuration (Nginx Reverse Proxy)

```nginx
server {
    listen 443 ssl http2;
    server_name app.adapteros.example.com;

    ssl_certificate /etc/letsencrypt/live/adapteros.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/adapteros.example.com/privkey.pem;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000" always;
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;

    # Large uploads (datasets) - adjust as needed
    client_max_body_size 2g;

    # Streaming endpoints (SSE + fetch streaming) must not be buffered
    location ~ ^/api/v1/(infer/stream|stream/|datasets/upload/progress) {
        proxy_pass http://localhost:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
        gzip off;
        add_header X-Accel-Buffering no;
    }

    # Dataset uploads - avoid buffering request bodies to disk
    location ^~ /api/v1/datasets/upload {
        proxy_pass http://localhost:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        proxy_request_buffering off;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }

    location / {
        proxy_pass http://localhost:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}

# Redirect HTTP to HTTPS
server {
    listen 80;
    server_name app.adapteros.example.com;
    return 301 https://$server_name$request_uri;
}
```

### Rate Limiting

Configure in `configs/cp-production.toml`:

```toml
[security.rate_limiting]
enabled = true
requests_per_minute = 60
burst = 10
```

### Backup and Recovery

**Backup JWT Keys**:

```bash
# Create encrypted backup
mkdir -p /backup/adapteros/jwt-keys/$(date +%Y%m%d)
cp var/jwt_*.pem /backup/adapteros/jwt-keys/$(date +%Y%m%d)/
tar czf - /backup/adapteros/jwt-keys/$(date +%Y%m%d)/ | \
  openssl enc -aes-256-cbc -salt -out /backup/adapteros/jwt-keys-$(date +%Y%m%d).tar.gz.enc
```

**Key Rotation Procedure:**

1. Generate new keypair
2. Configure old public key for validation
3. Configure new private key for signing
4. Wait for all old tokens to expire
5. Remove old public key

---

### Performance Standards

**Tenant Isolation Performance (Migration 0210)**

All tenant-scoped queries must meet strict SLAs to ensure scalability:

| Operation | SLA Target | Required Index |
|-----------|------------|----------------|
| Adapter Listing | < 10ms | `idx_adapters_tenant_active_tier_created` |
| Hash Lookup | < 1ms | `idx_adapters_tenant_hash_active_covering` |
| Doc Pagination | < 10ms | `idx_documents_tenant_created` |
| Chat History | < 10ms | `idx_chat_messages_tenant_created` |

**Verification:**
- Run `./scripts/monitor_index_performance.sh` pre-deployment.
- Ensure no `SCAN TABLE` or `USE TEMP B-TREE` in output.
- Verify `ANALYZE` has been run on production DB.

## Scaling Guidelines

### Worker Pool Sizing

Formula:

```
workers = min(physical_cores, max_concurrent_requests / avg_latency_secs)
```

Example for M3 Max (16 cores):
- Target: 100 requests/sec
- Avg latency: 0.2 sec
- Workers needed: 100 * 0.2 = 20 workers
- Use: 16 workers (limited by cores)

### Memory Allocation

Per model:
- Base model (Qwen 2.5 7B int4): ~5GB
- K=3 adapters (16 rank): ~150MB per adapter = 450MB
- Headroom (15%): ~820MB
- **Total per model: ~6.3GB**

For 32GB system:
- Max concurrent models: 4
- Reserve 8GB for system
- Per-model memory: 6GB

### Adapter Eviction Strategy

Configure in `production.toml`:

```toml
[memory]
min_headroom_pct = 15
max_adapters_per_tenant = 20
evict_order = [
  "ephemeral_ttl",     # Expire directory-specific adapters first
  "cold_lru",          # Then least-recently-used cold adapters
  "warm_lru",          # Then warm adapters
  "framework_priority" # Preserve framework adapters
]

[eviction_policy]
ephemeral_ttl_hours = 24
cold_threshold_mins = 60
warm_threshold_mins = 15
```

### Horizontal Scaling

Add worker nodes with federation enabled:

```bash
# On new worker node
export DATABASE_URL="postgresql://adapteros:password@leader-ip/adapteros_prod"
export AOS_FEDERATION_MODE=cluster
export AOS_NODE_ID=node4

./target/release/adapteros-server --config configs/worker.toml
```

### Vertical Scaling

Increase GPU memory allocation in configs:

```toml
[memory]
min_headroom_pct = 10  # Allow more memory usage
max_adapters_per_tenant = 30

[router]
k_sparse = 5  # Activate more adapters simultaneously
```

---

## Production Checklist

### Security

- [ ] JWT secrets rotated and stored securely
- [ ] Packet filter (PF) rules enabled and tested
- [ ] Zero network egress verified with tcpdump
- [ ] TLS certificates installed for external access
- [ ] Database credentials rotated
- [ ] Ed25519 keypairs generated for signing
- [ ] RBAC roles configured (admin, operator, SRE)

### Database

- [ ] PostgreSQL tuning applied:
  ```sql
  ALTER SYSTEM SET shared_buffers = '4GB';
  ALTER SYSTEM SET effective_cache_size = '12GB';
  ALTER SYSTEM SET maintenance_work_mem = '1GB';
  ALTER SYSTEM SET checkpoint_completion_target = 0.9;
  ALTER SYSTEM SET wal_buffers = '16MB';
  ALTER SYSTEM SET default_statistics_target = 100;
  ```
- [ ] pgvector indices created:
  ```sql
  CREATE INDEX ON rag_documents USING hnsw (embedding vector_cosine_ops);
  ```
- [ ] Automated backups configured
- [ ] Point-in-time recovery tested
- [ ] Connection pooling configured (pool_size=20)

### Monitoring

- [ ] Prometheus scraping `/metrics` endpoint
- [ ] Grafana dashboards imported
- [ ] Alert rules configured:
  ```yaml
  - alert: HighInferenceLatency
    expr: histogram_quantile(0.95, adapteros_inference_latency_seconds) > 0.1
    for: 5m
    annotations:
      summary: "Inference p95 latency exceeds 100ms"

  - alert: HighMemoryUsage
    expr: adapteros_memory_usage_bytes / adapteros_memory_total_bytes > 0.85
    for: 2m
    annotations:
      summary: "Memory usage above 85%"
  ```
- [ ] Telemetry bundles rotating (max 500k events)
- [ ] Health checks returning 200 OK

### Policies

- [ ] All 25 policy packs enabled
- [ ] Compliance reports generated weekly
- [ ] Audit trail retention configured (90 days)
- [ ] Incident freeze procedures tested
- [ ] Rollback procedures documented and tested

### Performance

- [ ] Baseline inference latency recorded (p50/p95/p99)
- [ ] Throughput target met (tokens/sec)
- [ ] Memory headroom ≥15% maintained
- [ ] Queue depth < 1000 under normal load
- [ ] Golden runs established for regression testing

---

## Troubleshooting

### Authentication Issues

**Issue: "unauthorized" errors in production**

**Solutions:**

```bash
# Verify configuration
cat configs/cp-production.toml | grep -A5 "\[auth\]"

# Check JWT key files exist and have correct permissions
ls -l var/jwt_*.pem

# Test with curl
curl -v https://app.adapteros.example.com/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "admin@example.com", "password": "password"}'

# Check server logs
tail -f /var/log/adapteros/server.log | grep -i auth
```

**Issue: CORS errors in browser**

**Solution:**

```toml
# Update configs/cp-production.toml
[security]
cors_origins = [
    "https://app.adapteros.example.com",
    "https://app.adapteros.example.com:443"  # Include port if needed
]
```

### Build and Compilation Issues

**Issue: Metal Kernel Compilation Failures**

```bash
# Rebuild Metal kernels
cd metal && bash build.sh

# Check for Xcode command line tools
xcode-select --install

# Verify metal compiler
xcrun -sdk macosx metal --version
```

**Issue: Rust Compilation Errors**

```bash
# Clean and rebuild
cargo clean && cargo build --release

# Check Rust version
rustc --version  # Should be 1.75+

# Update Rust
rustup update stable

# Check feature flags
cargo build --release --features rag-pgvector
```

### Database Issues

**Issue: Database Connection Issues**

```bash
# Check database file permissions
ls -la var/aos-cp.sqlite3

# Reinitialize database
rm var/aos-cp.sqlite3
./aosctl db migrate

# For PostgreSQL: check connection
psql -h localhost -U adapteros -d adapteros_prod
```

**Issue: Migration Failures**

```bash
# Check migration status
./aosctl db status

# Verify migration signatures
cat migrations/signatures.json

# Re-run migrations
./aosctl db migrate --force
```

### Worker Issues

**Issue: Worker Startup Failures**

```bash
# Check socket permissions
ls -la /var/run/aos/production/

# Verify tenant exists
./aosctl list-tenants

# Check worker logs
tail -f var/logs/worker.log

# Verify backend availability
./aosctl backend-info
```

**Issue: Inference Timeouts**

```bash
# Check worker health
./aosctl worker-status --tenant production

# Verify model loading
./aosctl model-info --tenant production

# Check memory usage
./aosctl system-metrics

# Increase timeout in config
[worker.safety]
inference_timeout_secs = 60  # Increase from 30
```

### Port Conflicts

```bash
# Clean up ports
./scripts/fresh-build.sh  # Stops services, cleans ports

# Manual port cleanup
lsof -ti:8080 | xargs kill
lsof -ti:3200 | xargs kill

# Check what's using a port
lsof -i :8080
```

### Performance Issues

**Issue: High Memory Usage**

```bash
# Check memory metrics
./aosctl system-metrics

# Reduce adapter cache
[memory]
max_adapters_per_tenant = 10  # Reduce from 20
min_headroom_pct = 20  # Increase headroom

# Force eviction
./aosctl evict-adapters --tenant production
```

**Issue: Slow Inference**

```bash
# Profile inference
./aosctl infer --prompt "test" --profile

# Check backend performance
./aosctl backend-bench --backend mlx

# Optimize router
[router]
k_sparse = 2  # Reduce from 3
```

### Logging and Debugging

Enable debug logging:

```bash
# Set environment variables
export RUST_LOG=debug
export RUST_BACKTRACE=1

# Run with verbose output
./aosctl serve --tenant production --plan my-plan --verbose

# Check logs
tail -f var/logs/server.log
tail -f /var/log/adapteros/telemetry.jsonl
```

### Determinism Issues

1. Check seed derivation (same inputs → same seeds)
2. Verify router sorting (score DESC, index ASC tie-break)
3. Confirm Q15 denominator = 32767.0
4. Run `cargo test --test determinism_core_suite -- --test-threads=8`, `cargo test -p adapteros-lora-router --test determinism`, and `bash scripts/check_fast_math_flags.sh`

### Health Check Failures

```bash
# Check control plane health
curl http://localhost:8080/healthz

# Check worker status
./aosctl worker-status --tenant production

# Monitor system metrics with interval
./aosctl system-metrics --interval 30
```

---

## References

- [README.md](../README.md) - Quick start and feature overview
- [AGENTS.md](../AGENTS.md) - Development guide
- [docs/AUTHENTICATION.md](AUTHENTICATION.md) - Authentication details
- [docs/POLICIES.md](POLICIES.md) - 25 policy packs
- [docs/DETERMINISM.md](DETERMINISM.md) - Determinism and replay guarantees

---

**Version:** 2.0
**Last Updated:** 2025-01-15
**Consolidated from:** DEPLOYMENT.md, deployment-guide.md
**Maintained By:** adapterOS Team
