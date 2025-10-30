# AdapterOS Deployment Guide

**Complete production deployment guide with multi-node setup, Kubernetes orchestration, air-gapped deployment, and scaling guidelines.**

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Installation Methods](#installation-methods)
3. [Single-Node Production Setup](#single-node-production-setup)
4. [Authentication Configuration](#authentication-configuration)
5. [Multi-Node Cluster Setup](#multi-node-cluster-setup)
6. [Kubernetes Deployment](#kubernetes-deployment)
7. [Air-Gapped Deployment](#air-gapped-deployment)
8. [Scaling Guidelines](#scaling-guidelines)
9. [Production Checklist](#production-checklist)
10. [Troubleshooting](#troubleshooting)

---

## Prerequisites

### Hardware Requirements
- **macOS 13.0+** with Apple Silicon (M1/M2/M3/M4)
- **RAM**: ≥16GB (32GB+ recommended for production)
- **Disk**: ≥100GB free space
- **Network**: Gigabit Ethernet for multi-node

### Software Requirements
- **Rust 1.75+**: `rustup install stable`
- **PostgreSQL 15+** with pgvector extension
- **Metal SDK**: Included with Xcode Command Line Tools
- **Docker** (for containerized deployments)

---

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

### Option 2: Manual Installation

```bash
# Clone the repository
git clone https://github.com/rogu3bear/adapter-os.git
cd adapter-os

# Build the workspace
cargo build --release

# Initialize the database
./target/release/aosctl init-tenant --id default --uid 1000 --gid 1000
```

---

## Single-Node Production Setup

### 1. Database Configuration

#### PostgreSQL with pgvector

```bash
# Install PostgreSQL with pgvector
brew install postgresql@15
brew install pgvector

# Start PostgreSQL
brew services start postgresql@15

# Create database
createdb adapteros_prod

# Enable pgvector extension
psql adapteros_prod -c "CREATE EXTENSION vector;"
```

#### Configure Environment

```bash
# Set database URL
export DATABASE_URL="postgresql://localhost/adapteros_prod"

# RAG embedding dimension (must match model)
export RAG_EMBED_DIM=3584

# Set adapter storage path
export AOS_ADAPTERS_ROOT=/var/lib/adapteros/adapters
```

### 2. Build AdapterOS

```bash
# Clone repository
git clone https://github.com/rogu3bear/adapter-os.git
cd adapter-os

# Build with production features
cargo build --release --features rag-pgvector

# Install binaries
sudo cp target/release/aosctl /usr/local/bin/
sudo cp target/release/adapteros-server /usr/local/bin/
```

### 3. Initialize System

```bash
# Run database migrations
aosctl db migrate

# Initialize default tenant
aosctl init-tenant --id production --uid 1000 --gid 1000

# Import base model
aosctl import-model \
  --name qwen2.5-7b-instruct \
  --weights models/qwen2.5-7b-mlx/weights.safetensors \
  --config models/qwen2.5-7b-mlx/config.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json
```

### 4. Configure Production Settings

Create `configs/production.toml`:

```toml
[server]
port = 8080
bind_address = "0.0.0.0"
workers = 4

[db]
url = "postgresql://localhost/adapteros_prod"
pool_size = 20

[security]
require_pf_deny = true  # Enforce packet filter rules
jwt_secret_path = "/etc/adapteros/jwt.secret"

[paths]
plan_dir = "/var/lib/adapteros/plans"
artifact_dir = "/var/lib/adapteros/artifacts"
adapters_root = "/var/lib/adapteros/adapters"

[router]
k_sparse = 3
entropy_floor = 0.02
gate_quant = "q15"

[memory]
min_headroom_pct = 15
evict_order = ["ephemeral_ttl", "cold_lru", "warm_lru"]

[telemetry]
enabled = true
json_output = "/var/log/adapteros/telemetry.jsonl"
prometheus_port = 9090

[policies]
# Enable all 22 policy packs
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
# Note: All adapteros-* crates enabled
```

### 5. Start Services

```bash
# Start server with production config
adapteros-server --config configs/production.toml
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

### Environment Configurations

#### Development Environment

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

#### Production Environment

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

**Generate Production Keys**:
```bash
# Generate Ed25519 keypair (recommended for production)
openssl genpkey -algorithm Ed25519 -out var/jwt_private.pem
openssl pkey -in var/jwt_private.pem -pubout -out var/jwt_public.pem

# Set restrictive permissions
chmod 600 var/jwt_private.pem
chmod 644 var/jwt_public.pem
```

### JWT Configuration Modes

#### HMAC Mode (Simple, Shared Secret)
**Use case:** Single-server deployments

```bash
# Generate secret
openssl rand -base64 32 > var/jwt_secret.key

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

# Set in config
[jwt]
mode = "eddsa"
private_key_file = "var/jwt_private.pem"
public_key_file = "var/jwt_public.pem"
```

### Security Hardening

#### HTTPS Configuration (Nginx Reverse Proxy)

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

    location / {
        proxy_pass http://localhost:8080;
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

#### Rate Limiting

Configure in `configs/cp-production.toml`:
```toml
[security.rate_limiting]
enabled = true
requests_per_minute = 60
burst = 10
```

**Citation:** [source: crates/adapteros-server-api/src/rate_limit.rs] - Per-tenant token-bucket rate limiting

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

**For detailed authentication troubleshooting:** See [docs/AUTHENTICATION.md](AUTHENTICATION.md) and [docs/DEPLOYMENT_AUTH.md](DEPLOYMENT_AUTH.md) (archived).

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

# Edit pg_hba.conf (add nodes 2 and 3)
host    adapteros_prod    adapteros    192.168.1.0/24    md5

# Create database user
psql -c "CREATE USER adapteros WITH PASSWORD 'secure_password';"
psql -c "GRANT ALL PRIVILEGES ON DATABASE adapteros_prod TO adapteros;"

# Restart PostgreSQL
brew services restart postgresql@15
```

### 2. Worker Node Configuration

On **Node 2 and Node 3**:

```bash
# Set database URL to point to leader
export DATABASE_URL="postgresql://adapteros:secure_password@192.168.1.10/adapteros_prod"

# Set federation mode
export AOS_FEDERATION_MODE=cluster

# Set node ID
export AOS_NODE_ID=node2  # or node3

# Build and start worker
cargo build --release --features rag-pgvector
./target/release/adapteros-server --config configs/worker.toml
```

Worker config (`configs/worker.toml`):

```toml
[server]
port = 8081  # Different port per worker
bind_address = "0.0.0.0"
workers = 8

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

### 3. AdapterOS Deployment

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
```

---

## Air-Gapped Deployment

> **Note:** For detailed air-gapped installation, see [docs/deployment-guide.md](deployment-guide.md) (archived for reference).

### 1. Prepare Offline Bundle

On a machine with internet access:

```bash
# Clone repository with vendored dependencies
git clone --recursive https://github.com/rogu3bear/adapter-os.git
cd adapter-os

# Vendor Rust dependencies
cargo vendor

# Build offline
cargo build --release --offline

# Package everything
tar czf adapteros-offline-bundle.tar.gz \
  target/release/aosctl \
  target/release/adapteros-server \
  models/ \
  configs/ \
  metal/
```

### 2. Transfer and Install

On air-gapped machine:

```bash
# Extract bundle
tar xzf adapteros-offline-bundle.tar.gz

# Install binaries
sudo cp target/release/* /usr/local/bin/

# Set up local database
brew install postgresql@15
createdb adapteros_airgap
```

### 3. Configure Zero Egress

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

### 4. Packet Filter Rules

Create `/etc/pf.anchors/adapteros`:

```
# Block all network traffic from AdapterOS process
block drop out proto tcp from any to any user adapteros
block drop out proto udp from any to any user adapteros
block drop out proto icmp from any to any user adapteros

# Allow localhost only
pass out proto tcp from 127.0.0.1 to 127.0.0.1 user adapteros
pass out on lo0 user adapteros
```

Enable rules:

```bash
# Load anchor
sudo pfctl -f /etc/pf.conf
sudo pfctl -a adapteros -f /etc/pf.anchors/adapteros
sudo pfctl -e

# Verify with tcpdump (should show no outbound traffic)
sudo tcpdump -i any -n src host <server_ip>
```

---

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

---

## Production Checklist

### Scaling Guidelines
- Horizontal: Add worker nodes with federation enabled
- Vertical: Increase GPU memory allocation in configs

### Security Hardening
- Enable JWT rotation in auth config
- Set RBAC policies in cp.toml

### Monitoring Setup
- Configure Prometheus export
- Set up alerting thresholds

### Backup Strategies
- Daily DB snapshots
- Adapter registry backups

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

- [ ] All 22 policy packs enabled
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

## References

- [README.md](../README.md) - Quick start and feature overview
- [docs/control-plane.md](control-plane.md) - API endpoints
- [docs/rag-pgvector.md](rag-pgvector.md) - Vector database setup
- [docs/POLICIES.md](POLICIES.md) - 22 policy packs

---

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

### Deployment Issues

**For detailed troubleshooting:** See archived [docs/DEPLOYMENT_AUTH.md](DEPLOYMENT_AUTH.md) for authentication-specific issues and [docs/deployment-guide.md](deployment-guide.md) for air-gapped installation details.

---

**Version:** 1.1  
**Last Updated:** 2025-01-15  
**Consolidated from:** DEPLOYMENT.md, DEPLOYMENT_AUTH.md, deployment-guide.md  
**Maintained By:** AdapterOS Team
