# Cursor Integration Guide

## Overview

This guide explains how to integrate AdapterOS with Cursor IDE to provide local, deterministic, evidence-grounded code intelligence.

## Prerequisites

- AdapterOS runtime built and running
- Rust 1.75+ and Apple Silicon (macOS 13+)
- Base model (Qwen 2.5 7B) imported
- Tenant initialized

## Architecture

```
Cursor IDE
    ↓ (HTTP/JSON API)
AdapterOS Control Plane (:8080)
    ↓ (Code Job Manager)
CodeGraph + Router + RAG
    ↓ (5-Tier Adapter Hierarchy)
Evidence-Grounded Responses
```

### Five-Tier Adapter Hierarchy

1. **Base LLM**: Qwen 2.5 7B (int4)
2. **Code Layer**: General coding knowledge (rank 16)
3. **Framework Layer**: Django, React, etc. (rank 8-16)
4. **Repository Layer**: Project-specific adapters (rank 16-32)
5. **Ephemeral Layer**: Recent commit changes (rank 4-8, TTL 24-72h)

## Step 1: Prepare Runtime

### Build AdapterOS

```bash
# Clone and build
git clone https://github.com/yourorg/adapter-os
cd adapter-os
cargo build --release

# Or use installer
make installer
```

### Initialize Tenant

```bash
# Create tenant
./target/release/aosctl init-tenant \
  --id default \
  --uid 1000 \
  --gid 1000

# Import base model
./target/release/aosctl import-model \
  --name qwen2.5-7b \
  --weights models/qwen2.5-7b-mlx/weights.safetensors \
  --config models/qwen2.5-7b-mlx/config.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json
```

## Step 2: Enable Code Intelligence

### Register Your Repositories

```bash
# Initialize repository
aosctl code-init /path/to/your/project --tenant default

# Trigger initial scan
aosctl code-update your-project-name

# Check scan status
aosctl code-list
aosctl code-status your-project-name
```

### What the Scan Creates

- **CodeGraph**: Parsed AST with symbol table and call graph
- **Symbol Index**: SQLite FTS5 for fast lookup
- **Vector Index**: HNSW for semantic search
- **Test Map**: File and symbol coverage mapping
- **Framework Detection**: Automatic detection of Django, React, etc.

## Step 3: Use with Cursor (Base-only or with Synthetic Directory Adapter)

### Base-only (no adapters)
- Ensure the control plane is running: API at `http://127.0.0.1:8080/api`
- Cursor can target the OpenAI-compatible endpoints:
  - List models: `GET /api/v1/models`
  - Chat: `POST /api/v1/chat/completions` with model `adapteros-qwen2.5-7b`

### Optional: Upsert a synthetic directory adapter
You can synthesize a directory-scoped adapter (no training) and optionally activate it.

```bash
aosctl adapter directory-upsert \
  --tenant default \
  --root /absolute/path/to/repo \
  --path src \
  --activate
```

API equivalent:

```bash
curl -X POST http://127.0.0.1:8080/api/v1/adapters/directory/upsert \
  -H 'Content-Type: application/json' \
  -d '{
    "tenant_id": "default",
    "root": "/absolute/path/to/repo",
    "path": "src",
    "activate": true
  }'
```

This creates a placeholder artifact under `./adapters/{b3hash}.safetensors`, registers it, and (if requested) loads it into the lifecycle.

## Step 3: Configure Manifests

### Create Manifest with Code Features

```yaml
# configs/code-manifest.yaml
version: "4"
tenant_id: "default"
base_model: "qwen2.5-7b"

adapters:
  - id: "code_general"
    category: "code"
    scope: "global"
    rank: 16
    tier: "persistent"
  
  - id: "framework_django"
    category: "framework"
    scope: "framework"
    framework: "django"
    rank: 12
    tier: "persistent"
  
  - id: "repo_myproject"
    category: "codebase"
    scope: "repository"
    repo_binding: "myproject"
    rank: 24
    tier: "persistent"

router:
  k_sparse: 3
  code_features:
    enabled: true
    lang_detection: true
    framework_priors: true
    symbol_hits: true
    path_tokens: true

policies:
  code:
    path_allow: ["src/**", "tests/**"]
    path_deny: [".git/**", "node_modules/**"]
    evidence_min_spans: 1
    secret_patterns:
      - "(?i)(password|secret|key|token)\\s*=\\s*['\"]\\w+"
```

### Build and Serve Plan

```bash
# Build plan
aosctl build-plan \
  --manifest configs/code-manifest.yaml \
  --output plan/code-plan

# Serve plan
aosctl serve --plan-id code-plan-001
```

## Step 4: API Integration

### Available Endpoints

#### Register Repository
```bash
POST /v1/code/register-repo
{
  "tenant_id": "default",
  "repo_id": "myproject",
  "path": "/Users/dev/myproject",
  "languages": ["Python", "TypeScript"],
  "default_branch": "main"
}
```

#### Trigger Scan
```bash
POST /v1/code/scan
{
  "tenant_id": "default",
  "repo_id": "myproject",
  "commit": "abc123",
  "full_scan": true
}
```

#### Get Scan Status
```bash
GET /v1/code/scan/{job_id}
```

#### List Repositories
```bash
GET /v1/code/repositories?tenant_id=default
```

#### Get Repository Details
```bash
GET /v1/code/repositories/{repo_id}?tenant_id=default
```

#### Create Commit Delta
```bash
POST /v1/code/commit-delta
{
  "tenant_id": "default",
  "repo_id": "myproject",
  "base_commit": "abc123",
  "head_commit": "def456"
}
```

#### Subscribe to File Changes (SSE)
```bash
GET /v1/streams/file-changes?repo_id=myproject
```

### Example Cursor Integration (Python)

```python
import requests
import sseclient

# Register repository
response = requests.post(
    "http://localhost:8080/v1/code/register-repo",
    json={
        "tenant_id": "default",
        "repo_id": "myproject",
        "path": "/Users/dev/myproject",
        "languages": ["Python"],
        "default_branch": "main"
    }
)

# Trigger scan
scan_response = requests.post(
    "http://localhost:8080/v1/code/scan",
    json={
        "tenant_id": "default",
        "repo_id": "myproject",
        "commit": "HEAD",
        "full_scan": True
    }
)

job_id = scan_response.json()["job_id"]

# Poll for completion
while True:
    status = requests.get(f"http://localhost:8080/v1/code/scan/{job_id}").json()
    if status["status"] in ["completed", "failed"]:
        break
    time.sleep(2)

# Subscribe to file changes
messages = sseclient.SSEClient("http://localhost:8080/v1/streams/file-changes?repo_id=myproject")
for msg in messages:
    if msg.data:
        event = json.loads(msg.data)
        print(f"File changed: {event['file_path']} ({event['change_type']})")
```

## Step 5: Testing & Validation

### Test Repository Registration

```bash
# Register test repo
aosctl code-init . --tenant default

# Verify registration
aosctl code-list --json | jq '.repos[] | select(.repo_id == "adapter-os")'
```

### Test Scan Job

```bash
# Trigger scan
aosctl code-update adapter-os

# Should output:
# ✓ Scan job created: <job-id>
# Waiting for scan to complete...
# Progress: 10% (parse_and_build_graph)
# Progress: 50% (store_artifacts)
# Progress: 70% (index_symbols)
# Progress: 100% (complete)
# ✓ Scan completed successfully
#   Files: 500, Symbols: 5000
```

### Test SSE Streaming

```bash
# Terminal 1: Subscribe to file changes
curl -N http://localhost:8080/v1/streams/file-changes?repo_id=adapter-os

# Terminal 2: Make file changes
echo "// test" >> src/test.rs

# Terminal 1 should show:
# data: {"file_path":"src/test.rs","change_type":"modify","repo_id":"adapter-os"}
```

### Test Evidence-Grounded Response

```bash
# Issue inference request with code context
curl -X POST http://localhost:8080/v1/infer \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "How does the router select adapters?",
    "context": {
      "repo_id": "adapter-os",
      "file_path": "crates/adapteros-lora-router/src/lib.rs"
    }
  }'

# Response should include:
# {
#   "text": "The router selects adapters using...",
#   "trace": {
#     "evidence": [
#       {
#         "doc_id": "adapter-os:abc123:src/router.rs",
#         "span": "45:60",
#         "relevance": 0.92
#       }
#     ],
#     "router_summary": {
#       "selected_adapters": ["code_general", "repo_adapteros"],
#       "gates": [0.8, 0.6]
#     }
#   }
# }
```

## Troubleshooting

### Scan Jobs Failing

**Problem**: Scan jobs stuck in "pending" or failing

**Solution**:
1. Check code job manager is initialized in server
2. Verify artifact storage path exists: `mkdir -p var/artifacts`
3. Check logs: `tail -f var/logs/server.log`
4. Verify database migration: `sqlite3 var/cp.db ".schema repositories"`

### No File Change Events

**Problem**: SSE endpoint not streaming file changes

**Solution**:
1. Verify file_change_tx is initialized in AppState
2. Check Git subsystem is enabled
3. Verify repository is registered
4. Check SSE connection: `curl -v http://localhost:8080/v1/streams/file-changes`

### Router Not Using Code Features

**Problem**: Router not selecting repository-specific adapters

**Solution**:
1. Verify manifest includes `router.code_features.enabled: true`
2. Check adapter metadata includes `repo_binding`
3. Verify CodeGraph scan completed
4. Check router logs for feature extraction

### Repository Not Found

**Problem**: API returns 404 for repository operations

**Solution**:
1. Verify repository registered: `aosctl code-list`
2. Check tenant_id matches
3. Verify database entry: `sqlite3 var/cp.db "SELECT * FROM repositories;"`

## Performance Tuning

### Scan Performance

- **Small repos** (<1K files): ~10-30s
- **Medium repos** (1K-10K files): ~30-120s
- **Large repos** (>10K files): ~2-5min

Tune scan performance:
- Exclude directories: Update manifest `path_deny`
- Parallel parsing: Set `RAYON_NUM_THREADS`
- Incremental scans: Use commit delta instead of full scan

### Router Overhead

Target: ≤8% overhead at K=3

Optimize:
- Enable feature caching
- Reduce symbol index lookups
- Use path token bloom filters

### Memory Usage

- CodeGraph: ~10MB per 10K files
- Symbol index: ~5MB per 10K symbols
- Vector index: ~50MB per 10K chunks

## Next Steps

1. **Implement Ephemeral Adapters**: Hot-attach commit-specific adapters
2. **Enable Auto-Apply**: Configure patch auto-application policies
3. **Add Framework Adapters**: Train adapters for your frameworks
4. **Configure Policies**: Set path restrictions and secret detection
5. **Integrate with CI/CD**: Automate scans on commit hooks

## References

- [Architecture Documentation](./code-intelligence/code-intelligence-architecture.md)
- [API Registry](./code-intelligence/code-api-registry.md)
- [Router Features](./code-intelligence/code-router-features.md)
- [Implementation Roadmap](./code-intelligence/code-implementation-roadmap.md)

