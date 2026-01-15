# Model Management Guide

## Overview

adapterOS manages base models throughout their complete lifecycle: discovery, import, validation, loading, and unloading. Models are registered in the database with content-addressed hashes for integrity verification, and the system tracks load state across tenants with memory pressure awareness.

The model management system supports:
- Multiple model formats with backend auto-detection
- BLAKE3 hash verification for deterministic integrity
- Multi-tenant isolation with global model sharing
- HuggingFace Hub integration for model downloads
- Memory-aware loading with ANE (Apple Neural Engine) tracking

## Model Formats

adapterOS automatically detects model format from directory contents and selects the appropriate backend:

| Extension | Format | Backend | Description |
|-----------|--------|---------|-------------|
| `.safetensors` | safetensors | MLX | Default format for MLX inference (recommended) |
| `.gguf` | gguf | Metal | Quantized models for Metal GPU backend |
| `.mlpackage` | mlpackage | CoreML | Apple CoreML compiled models for ANE |

### Auto-Detection Logic

The system scans the model directory and applies these rules:
1. If any `.mlpackage` file exists, use CoreML backend
2. If any `.gguf` file exists, use Metal backend
3. Otherwise, default to safetensors format with MLX backend

```rust
// Detection priority (from crates/adapteros-server/src/model_seeding.rs)
// .mlpackage -> coreml backend (highest priority)
// .gguf -> metal backend
// default -> safetensors + mlx backend
```

## Importing Models

### CLI Import

Seed models from a local directory into the database:

```bash
# Seed models from AOS_MODEL_PATH environment variable
aosctl models seed

# Seed a specific model path
aosctl models seed --model-path ./var/models/Qwen2.5-7B-Instruct-4bit

# Force re-seed even if models already exist
aosctl models seed --force

# List registered models
aosctl models list

# Output as JSON
aosctl models list --json
```

The `models seed` command:
1. Scans the specified directory for model subdirectories containing `config.json`
2. Detects format and backend from file extensions
3. Computes BLAKE3 hashes for weights, config, and tokenizer files
4. Registers each model in the database with status `available`

### API Import

Import a model via the REST API:

```bash
POST /v1/models/import
Content-Type: application/json

{
  "model_name": "qwen-7b",
  "model_path": "/var/model-cache/models/qwen2.5-7b-instruct-bf16",
  "format": "mlx",
  "backend": "mlx",
  "capabilities": ["chat", "completion"],
  "metadata": {"architecture": "transformer"}
}
```

**Request Fields:**
- `model_name` (required): Human-readable name for the model
- `model_path` (required): Filesystem path to model directory
- `format` (required): One of `mlx`, `safetensors`, `pytorch`, `gguf`
- `backend` (required): One of `mlx`, `metal`, `coreml`
- `capabilities` (optional): Array of `chat`, `completion`, `embeddings`
- `metadata` (optional): Additional JSON metadata

**Response:**
```json
{
  "import_id": "01945a2b-3c4d-7e8f-9012-abcdef123456",
  "status": "available",
  "message": "Model import completed",
  "progress": 100
}
```

### Progress Tracking

Monitor active imports via the download progress endpoint:

```bash
GET /v1/models/download-progress
```

**Response:**
```json
{
  "schema_version": "1.0",
  "imports": [
    {
      "model_id": "01945a2b-...",
      "operation_id": "op-123",
      "operation": "import",
      "status": "in_progress",
      "started_at": "2025-01-02T10:00:00Z",
      "progress_pct": 45,
      "speed_mbps": 125.5,
      "eta_seconds": 120
    }
  ],
  "total_active": 1
}
```

## Model Validation

### Hash Verification

Models are validated using BLAKE3 cryptographic hashes. Each model stores hashes for:
- `hash_b3`: Combined hash of config + weights files
- `config_hash_b3`: Hash of `config.json`
- `tokenizer_hash_b3`: Hash of `tokenizer.json`
- `tokenizer_cfg_hash_b3`: Hash of `tokenizer_config.json`
- `license_hash_b3` (optional): Hash of license file

All hashes must be 64-character hexadecimal strings (256-bit BLAKE3 output).

### Validation Endpoint

Validate model integrity via the API:

```bash
GET /v1/models/{model_id}/validate
```

**Response:**
```json
{
  "model_id": "qwen-7b",
  "status": "ready",
  "valid": true,
  "can_load": true,
  "reason": null,
  "issues": [],
  "errors": []
}
```

**Validation Status Values:**
- `ready`: Model passes all validation checks
- `needs_setup`: Model requires additional configuration
- `invalid`: Model has validation errors

**Validation Checks:**
1. All required hashes are present (weights, config, tokenizer, tokenizer_config)
2. Hash format is valid (64-char hex string)
3. Metadata JSON is valid (if present)
4. License hash format is valid (if present)

### Metadata Requirements

Model metadata JSON should include architecture information:
```json
{
  "architecture": "qwen2",
  "num_hidden_layers": 28,
  "hidden_size": 3584,
  "vocab_size": 152064,
  "model_type": "qwen2"
}
```

This metadata is parsed for display in the UI and used for architecture-specific optimizations.

## Model Loading/Unloading

### Load Model

Load a model into memory for inference:

```bash
POST /v1/models/{model_id}/load
```

**Permissions Required:** Admin or Operator role

**Response:**
```json
{
  "model_id": "qwen-7b",
  "model_name": "Qwen 2.5 7B Instruct",
  "model_path": "/var/model-cache/models/qwen2.5-7b",
  "status": "ready",
  "loaded_at": "2025-01-02T10:30:00Z",
  "memory_usage_mb": 4096,
  "is_loaded": true,
  "ane_memory": {
    "allocated_mb": 2048,
    "used_mb": 1536,
    "available_mb": 512,
    "usage_pct": 75.0
  },
  "uma_pressure_level": "nominal"
}
```

**Load Status Values:**
- `ready` / `loaded`: Model is loaded and ready for inference
- `loading`: Model load in progress
- `unloading`: Model unload in progress
- `unloaded` / `no-model`: Model is not loaded
- `error`: Load failed

### Unload Model

Free memory by unloading a model:

```bash
POST /v1/models/{model_id}/unload
```

**Permissions Required:** Admin or Operator role

### Get Model Status

Check current load state:

```bash
GET /v1/models/{model_id}/status
```

**Permissions Required:** Any authenticated user

### List All Model Statuses

Get status for all models across the system:

```bash
GET /v1/models/status/all?tenant_id=default
```

**Permissions Required:** Operator, Admin, or Viewer role

**Query Parameters:**
- `tenant_id` (optional): Filter to specific tenant (admin only)

### Memory Estimation

The system tracks:
- `memory_usage_mb`: Total memory consumption of loaded model
- `ane_memory`: Apple Neural Engine memory allocation breakdown
- `uma_pressure_level`: Unified Memory Architecture pressure (`nominal`, `warning`, `critical`)

## Multi-Tenant Model Isolation

### Tenant Scoping

Models are scoped to their creating tenant by default:
- Models with `tenant_id` set are visible only to that tenant
- Global models (`tenant_id = NULL`) are visible to all tenants
- Cross-tenant access is denied with HTTP 404 (prevents enumeration)

### Visibility Rules

| Model tenant_id | Requesting Tenant | Access |
|-----------------|-------------------|--------|
| `tenant-a` | `tenant-a` | Allowed |
| `tenant-a` | `tenant-b` | Denied (404) |
| `NULL` (global) | Any tenant | Allowed |

### Admin Access

Admins can access cross-tenant model data when:
1. They have the `admin` role
2. The target tenant is in their `admin_tenants` claim list
3. They use the `tenant_id` query parameter explicitly

```bash
# Admin querying another tenant's models
GET /v1/models/status/all?tenant_id=tenant-a
Authorization: Bearer <admin-token>
```

### Role Permissions

| Operation | Viewer | Operator | Admin |
|-----------|--------|----------|-------|
| List models | Yes | Yes | Yes |
| Get status | Yes | Yes | Yes |
| Validate | Yes | Yes | Yes |
| Load/Unload | No | Yes | Yes |
| Import | No | Yes | Yes |
| Delete | No | No | Yes |

## HuggingFace Hub Integration

### Environment Variables

Configure HuggingFace Hub downloads:

```bash
# Enable HF Hub integration
export AOS_HF_HUB_ENABLED=true

# HuggingFace API token (for private/gated models)
export HF_TOKEN=hf_xxxxxxxxxxxxxxxxxxxxx

# Custom registry URL (default: https://huggingface.co)
export AOS_HF_REGISTRY_URL=https://huggingface.co

# Local cache directory (default: var/model-cache)
export AOS_MODEL_CACHE_DIR=/path/to/cache

# Maximum concurrent downloads (default: 4, range: 1-10)
export AOS_MAX_CONCURRENT_DOWNLOADS=4

# Download timeout in seconds (default: 300, range: 30-3600)
export AOS_DOWNLOAD_TIMEOUT_SECS=300
```

### Priority Models

Configure models to download automatically at server startup:

```bash
# Comma-separated list of HuggingFace repository IDs
export AOS_PRIORITY_MODELS="mlx-community/Qwen2.5-7B-Instruct-4bit,mlx-community/bge-small-en-v1.5"
```

Priority model downloads:
- Run during server boot (non-blocking)
- Failures are logged but do not prevent startup
- Models are cached locally after first download

### Token Authentication

For private or gated models, set the `HF_TOKEN` environment variable:

```bash
export HF_TOKEN=hf_xxxxxxxxxxxxxxxxxxxxx
```

The token is passed to HuggingFace API requests and enables access to:
- Private repositories
- Gated models (e.g., Llama 2, Gemma)
- Higher rate limits

## Troubleshooting

### MODEL_PATH_MISSING Error

**Error:** `model path does not exist: /path/to/model`

**Cause:** The model's filesystem path is not valid or accessible.

**Solutions:**
1. Verify the model path exists:
   ```bash
   ls -la /path/to/model
   ```

2. Set the correct cache directory:
   ```bash
   export AOS_MODEL_CACHE_DIR=/var/model-cache
   export AOS_BASE_MODEL_ID=Qwen2.5-7B-Instruct-4bit
   ```

3. Update the model path in the database:
   ```bash
   aosctl models seed --model-path /correct/path --force
   ```

4. Use the `--model-path` flag with preflight:
   ```bash
   aosctl preflight --model-path ./models/my-model
   ```

### Hash Verification Failures

**Error:** `Invalid weights hash format: expected 64-char hex`

**Cause:** BLAKE3 hashes must be 64-character hexadecimal strings.

**Solutions:**
1. Re-import the model to regenerate hashes:
   ```bash
   aosctl models seed --model-path /path/to/model --force
   ```

2. Verify file integrity:
   ```bash
   # Compute BLAKE3 hash manually
   b3sum /path/to/model/config.json
   ```

### Memory Pressure During Load

**Error:** Worker failed to load model / Memory pressure critical

**Cause:** Insufficient unified memory for model weights.

**Solutions:**
1. Check current memory usage:
   ```bash
   GET /v1/models/status/all
   # Look at uma_pressure_level field
   ```

2. Unload unused models:
   ```bash
   POST /v1/models/{other_model_id}/unload
   ```

3. Use quantized models (4-bit or 8-bit) to reduce memory footprint

4. Monitor ANE allocation:
   ```json
   {
     "ane_memory": {
       "allocated_mb": 8192,
       "used_mb": 7500,
       "available_mb": 692,
       "usage_pct": 91.5
     }
   }
   ```

### Worker Unavailable

**Error:** `no worker available` (503 WORKER_UNAVAILABLE)

**Cause:** The inference worker process is not running or not registered.

**Solutions:**
1. Check worker status:
   ```bash
   aosctl status
   ```

2. Start the worker:
   ```bash
   ./start
   # Or manually:
   cargo run -p adapteros-lora-worker -- --config configs/cp.toml
   ```

3. Verify worker socket path:
   ```bash
   export AOS_WORKER_SOCKET=/var/run/worker.sock
   ls -la /var/run/worker.sock
   ```

### Model Not Found in Listing

**Error:** Model imported but not appearing in `GET /v1/models`

**Cause:** Model import status is not `available`, or tenant isolation is blocking access.

**Solutions:**
1. Check import status in database:
   ```bash
   aosctl models list --json | jq '.models[] | {name, import_status}'
   ```

2. Verify tenant ownership:
   ```sql
   SELECT id, name, tenant_id, import_status FROM models WHERE name LIKE '%model-name%';
   ```

3. Complete pending imports:
   ```bash
   GET /v1/models/download-progress
   # Wait for imports to complete
   ```

## API Reference Summary

| Endpoint | Method | Description | Auth Required |
|----------|--------|-------------|---------------|
| `/v1/models` | GET | List all models with stats | Yes |
| `/v1/models/import` | POST | Import model from path | Admin/Operator |
| `/v1/models/download-progress` | GET | Get active import progress | Yes |
| `/v1/models/status/all` | GET | All model statuses | Operator+ |
| `/v1/models/{id}/load` | POST | Load model into memory | Operator+ |
| `/v1/models/{id}/unload` | POST | Unload model from memory | Operator+ |
| `/v1/models/{id}/status` | GET | Get model load status | Yes |
| `/v1/models/{id}/validate` | GET | Validate model integrity | Yes |
