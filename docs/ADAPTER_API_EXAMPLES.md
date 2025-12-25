# Adapter API Examples

**Version:** 1.0.0
**Last Updated:** 2025-12-24

This document provides comprehensive API examples for adapter operations in AdapterOS, including curl commands, request/response formats, and common workflows.

---

## Table of Contents

1. [Setup](#setup)
2. [Adapter Registration](#adapter-registration)
3. [List Adapters](#list-adapters)
4. [Get Adapter Details](#get-adapter-details)
5. [Load Adapter](#load-adapter)
6. [Unload Adapter](#unload-adapter)
7. [Hot-Swap Adapters](#hot-swap-adapters)
8. [Import Adapter](#import-adapter)
9. [Adapter Stacks](#adapter-stacks)
10. [Streaming Inference with Adapters](#streaming-inference-with-adapters)
11. [Adapter Lifecycle Management](#adapter-lifecycle-management)
12. [Adapter Repositories](#adapter-repositories)
13. [Adapter Versions](#adapter-versions)
14. [Complete Workflows](#complete-workflows)

---

## Setup

### Environment Variables

```bash
# Base configuration
export AOS_BASE_URL="${AOS_BASE_URL:-http://localhost:8080}"
export AOS_TOKEN="your-jwt-token-here"

# Tenant and resource IDs (replace with your actual IDs)
export AOS_TENANT_ID="tenant-123"
export AOS_ADAPTER_ID="adapter-abc123"
export AOS_STACK_ID="stack-xyz789"
export AOS_REPO_ID="repo-456"
```

### Authentication

All protected endpoints require a JWT token:

```bash
# Login to get token
curl -X POST "$AOS_BASE_URL/v1/auth/login" \
  -H "Content-Type: application/json" \
  -d '{
    "email": "admin@example.com",
    "password": "password"
  }' | jq -r '.token' > ~/.aos_token

# Export token for subsequent requests
export AOS_TOKEN=$(cat ~/.aos_token)
```

---

## Adapter Registration

### Register a New Adapter

**Endpoint:** `POST /v1/adapters/register`

**Required Permissions:** `AdapterRegister` (Admin or Operator role)

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapters/register" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "customer-support-v1",
    "description": "Fine-tuned for customer support queries",
    "framework": "pytorch",
    "base_model": "qwen2.5-7b-instruct",
    "lora_rank": 16,
    "lora_alpha": 32,
    "category": "customer_service",
    "scope": "support_tickets",
    "tier": "standard"
  }'
```

**Response (201 Created):**
```json
{
  "schema_version": "1.0",
  "id": "adapter-abc123",
  "adapter_id": "adapter-abc123",
  "name": "customer-support-v1",
  "hash_b3": "blake3:a1b2c3d4...",
  "rank": 16,
  "tier": "standard",
  "framework": "pytorch",
  "category": "customer_service",
  "scope": "support_tickets",
  "lifecycle_state": "registered",
  "runtime_state": "unloaded",
  "created_at": "2025-12-24T10:30:00Z",
  "version": "1"
}
```

**Error Codes:**
- `403 FORBIDDEN` - Insufficient permissions
- `400 VALIDATION_ERROR` - Invalid adapter name or parameters
- `409 CONFLICT` - Adapter name already exists for tenant

---

## List Adapters

### List All Adapters

**Endpoint:** `GET /v1/adapters`

**Required Permissions:** `AdapterList` (All roles)

**Request:**
```bash
curl -X GET "$AOS_BASE_URL/v1/adapters" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

### List Adapters with Filters

**Query Parameters:**
- `tier` - Filter by tier (e.g., "1", "2")
- `framework` - Filter by framework (e.g., "pytorch", "mlx")

```bash
# Filter by tier
curl -X GET "$AOS_BASE_URL/v1/adapters?tier=1" \
  -H "Authorization: Bearer $AOS_TOKEN"

# Filter by framework
curl -X GET "$AOS_BASE_URL/v1/adapters?framework=pytorch" \
  -H "Authorization: Bearer $AOS_TOKEN"

# Multiple filters
curl -X GET "$AOS_BASE_URL/v1/adapters?tier=1&framework=pytorch" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
[
  {
    "schema_version": "1.0",
    "id": "adapter-abc123",
    "adapter_id": "adapter-abc123",
    "name": "customer-support-v1",
    "hash_b3": "blake3:a1b2c3d4...",
    "rank": 16,
    "tier": "1",
    "framework": "pytorch",
    "category": "customer_service",
    "scope": "support_tickets",
    "lifecycle_state": "production",
    "runtime_state": "warm",
    "stats": {
      "total_activations": 1234,
      "selected_count": 987,
      "avg_gate_value": 0.85,
      "selection_rate": 80.0
    },
    "created_at": "2025-12-24T10:30:00Z",
    "updated_at": "2025-12-24T15:45:00Z",
    "version": "1"
  },
  {
    "schema_version": "1.0",
    "id": "adapter-def456",
    "adapter_id": "adapter-def456",
    "name": "code-review-assistant",
    "hash_b3": "blake3:e5f6g7h8...",
    "rank": 8,
    "tier": "1",
    "framework": "pytorch",
    "category": "code",
    "scope": "reviews",
    "lifecycle_state": "production",
    "runtime_state": "hot",
    "stats": {
      "total_activations": 567,
      "selected_count": 502,
      "avg_gate_value": 0.92,
      "selection_rate": 88.5
    },
    "created_at": "2025-12-20T08:15:00Z",
    "updated_at": "2025-12-24T14:20:00Z",
    "version": "2"
  }
]
```

---

## Get Adapter Details

### Get Specific Adapter

**Endpoint:** `GET /v1/adapters/{adapter_id}`

**Required Permissions:** `AdapterView` (All roles)

**Request:**
```bash
curl -X GET "$AOS_BASE_URL/v1/adapters/$AOS_ADAPTER_ID" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
{
  "schema_version": "1.0",
  "id": "adapter-abc123",
  "adapter_id": "adapter-abc123",
  "name": "customer-support-v1",
  "hash_b3": "blake3:a1b2c3d4e5f6...",
  "rank": 16,
  "tier": "1",
  "assurance_tier": null,
  "languages": ["en", "es"],
  "framework": "pytorch",
  "category": "customer_service",
  "scope": "support_tickets",
  "framework_id": "pytorch-2.1.0",
  "framework_version": "2.1.0",
  "repo_id": "repo-456",
  "commit_sha": "abc123def456",
  "intent": "Customer support query handling",
  "lora_tier": "standard",
  "lora_strength": 1.0,
  "lora_scope": "support_tickets",
  "created_at": "2025-12-24T10:30:00Z",
  "updated_at": "2025-12-24T15:45:00Z",
  "stats": {
    "total_activations": 1234,
    "selected_count": 987,
    "avg_gate_value": 0.85,
    "selection_rate": 80.0
  },
  "version": "1",
  "lifecycle_state": "production",
  "runtime_state": "warm",
  "pinned": false,
  "memory_bytes": 16777216
}
```

**Error Codes:**
- `404 NOT_FOUND` - Adapter doesn't exist
- `403 FORBIDDEN` - Cross-tenant access denied

---

## Load Adapter

### Load Adapter into Memory

**Endpoint:** `POST /v1/adapters/{adapter_id}/load`

**Required Permissions:** `AdapterLoad` (Operator or Admin role)

**Lifecycle Transitions:** `Unloaded → Cold → Warm`

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapters/$AOS_ADAPTER_ID/load" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
{
  "message": "Adapter 'customer-support-v1' loaded successfully",
  "adapter_id": "adapter-abc123",
  "lifecycle_state": "warm",
  "runtime_state": "warm",
  "memory_bytes": 16777216
}
```

**Error Codes:**
- `404 ADAPTER_NOT_FOUND` - Adapter doesn't exist
- `500 ADAPTER_NOT_LOADABLE` - Adapter can't be loaded (corrupted, missing files)
- `503 NO_COMPATIBLE_WORKER` - No workers available
- `507 STORAGE_QUOTA_EXCEEDED` - Memory limit reached

### Promote Adapter Lifecycle

**Endpoint:** `POST /v1/adapters/{adapter_id}/lifecycle/promote`

**Transitions:** `Unloaded → Cold → Warm → Hot → Resident`

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapters/$AOS_ADAPTER_ID/lifecycle/promote" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
{
  "adapter_id": "adapter-abc123",
  "previous_state": "warm",
  "new_state": "hot",
  "message": "Adapter promoted to Hot"
}
```

---

## Unload Adapter

### Unload Adapter from Memory

**Endpoint:** `POST /v1/adapters/{adapter_id}/unload`

**Required Permissions:** `AdapterUnload` (Operator or Admin role)

**Lifecycle Transitions:** `Resident → Hot → Warm → Cold → Unloaded`

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapters/$AOS_ADAPTER_ID/unload" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
{
  "message": "Adapter 'customer-support-v1' unloaded successfully",
  "adapter_id": "adapter-abc123",
  "lifecycle_state": "unloaded",
  "runtime_state": "unloaded",
  "memory_freed_bytes": 16777216
}
```

### Demote Adapter Lifecycle

**Endpoint:** `POST /v1/adapters/{adapter_id}/lifecycle/demote`

**Transitions:** `Resident → Hot → Warm → Cold → Unloaded`

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapters/$AOS_ADAPTER_ID/lifecycle/demote" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
{
  "adapter_id": "adapter-abc123",
  "previous_state": "hot",
  "new_state": "warm",
  "message": "Adapter demoted to Warm"
}
```

---

## Hot-Swap Adapters

### Atomically Swap Adapters

**Endpoint:** `POST /v1/adapters/swap`

**Required Permissions:** `AdapterLoad` AND `AdapterUnload` (Operator or Admin role)

**Description:** Atomically replaces one adapter with another with minimal downtime. Unloads the old adapter and loads the new one.

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapters/swap" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "old_adapter_id": "adapter-abc123",
    "new_adapter_id": "adapter-xyz789",
    "dry_run": false
  }'
```

**Request Schema:**
```json
{
  "old_adapter_id": "string",    // Adapter to unload
  "new_adapter_id": "string",    // Adapter to load
  "dry_run": false              // Optional: validate without executing
}
```

**Response (200 OK):**
```json
{
  "success": true,
  "message": "Adapter swap completed successfully",
  "old_adapter": {
    "id": "adapter-abc123",
    "name": "customer-support-v1",
    "runtime_state": "unloaded"
  },
  "new_adapter": {
    "id": "adapter-xyz789",
    "name": "customer-support-v2",
    "runtime_state": "warm"
  },
  "swap_duration_ms": 342
}
```

### Dry-Run Swap Validation

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapters/swap" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "old_adapter_id": "adapter-abc123",
    "new_adapter_id": "adapter-xyz789",
    "dry_run": true
  }'
```

**Response (200 OK):**
```json
{
  "success": true,
  "message": "Dry run: swap validated successfully",
  "validation": {
    "old_adapter_exists": true,
    "new_adapter_exists": true,
    "base_model_compatible": true,
    "memory_available": true
  }
}
```

**Error Codes:**
- `404 NOT_FOUND` - Old or new adapter doesn't exist
- `400 INVALID_REQUEST` - Adapters have incompatible base models
- `500 SWAP_FAILED` - Swap operation failed (old adapter unloaded but new adapter failed to load)

---

## Import Adapter

### Import Adapter from .aos File

**Endpoint:** `POST /v1/adapters/import`

**Required Permissions:** `AdapterRegister` (Admin or Operator role)

**Query Parameters:**
- `load` (optional, boolean) - Auto-load adapter after import (default: false)

**Request:**
```bash
# Import without auto-load
curl -X POST "$AOS_BASE_URL/v1/adapters/import" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -F "file=@/path/to/adapter.aos"

# Import with auto-load
curl -X POST "$AOS_BASE_URL/v1/adapters/import?load=true" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -F "file=@/path/to/adapter.aos"
```

**Response (200 OK):**
```json
{
  "schema_version": "1.0",
  "id": "adapter-imported-123",
  "adapter_id": "adapter-imported-123",
  "name": "imported-adapter",
  "hash_b3": "blake3:f1e2d3c4...",
  "rank": 16,
  "tier": "warm",
  "framework": "pytorch",
  "category": "general",
  "scope": "default",
  "lifecycle_state": "registered",
  "runtime_state": "warm",
  "created_at": "2025-12-24T16:30:00Z",
  "version": "1",
  "deduplicated": false,
  "memory_bytes": 16777216
}
```

**Deduplication Response:**

If an adapter with the same hash already exists, AdapterOS returns the existing adapter:

```json
{
  "schema_version": "1.0",
  "id": "adapter-existing-456",
  "adapter_id": "adapter-existing-456",
  "name": "existing-adapter",
  "hash_b3": "blake3:f1e2d3c4...",
  "runtime_state": "warm",
  "deduplicated": true,
  "message": "Adapter already exists with matching hash"
}
```

**Error Codes:**
- `413 PAYLOAD_TOO_LARGE` - File exceeds 500MB limit
- `400 INVALID_FILE` - File is not a valid .aos format
- `500 IMPORT_FAILED` - Import failed (disk full, corruption, etc.)

---

## Adapter Stacks

### Create Adapter Stack

**Endpoint:** `POST /v1/adapter-stacks`

**Required Permissions:** `AdapterRegister` (Admin or Operator role)

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapter-stacks" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "production-support-stack",
    "description": "Production customer support adapter stack",
    "adapter_ids": [
      "adapter-abc123",
      "adapter-def456",
      "adapter-ghi789"
    ],
    "workflow_type": "parallel",
    "determinism_mode": "strict",
    "routing_determinism_mode": "deterministic"
  }'
```

**Request Schema:**
```json
{
  "name": "string",                              // Stack name (required)
  "description": "string",                       // Stack description (optional)
  "adapter_ids": ["string"],                    // Array of adapter IDs (required)
  "workflow_type": "parallel",                  // parallel|sequential|upstream_downstream (optional)
  "metadata": {                                 // Optional metadata
    "dataset_version_id": "dataset-123"
  },
  "determinism_mode": "strict",                 // strict|besteffort|relaxed (optional)
  "routing_determinism_mode": "deterministic"   // deterministic|adaptive (optional)
}
```

**Response (201 Created):**
```json
{
  "schema_version": "1.0",
  "id": "stack-xyz789",
  "tenant_id": "tenant-123",
  "name": "production-support-stack",
  "description": "Production customer support adapter stack",
  "adapter_ids": [
    "adapter-abc123",
    "adapter-def456",
    "adapter-ghi789"
  ],
  "workflow_type": "parallel",
  "created_at": "2025-12-24T10:00:00Z",
  "updated_at": "2025-12-24T10:00:00Z",
  "is_active": false,
  "is_default": false,
  "version": 1,
  "lifecycle_state": "active",
  "warnings": [],
  "determinism_mode": "strict",
  "routing_determinism_mode": "deterministic"
}
```

**Capacity Warnings:**

If creating the stack would exceed memory or adapter limits, the API returns warnings:

```json
{
  "schema_version": "1.0",
  "id": "stack-xyz789",
  "warnings": [
    "High memory pressure detected (High): 15.2% headroom remaining. Consider reducing concurrent operations.",
    "Stack would exceed configured adapter limit: 55 adapters (limit: 50). Current: 45, Adding: 10"
  ],
  ...
}
```

**Error Codes:**
- `400 VALIDATION_ERROR` - Invalid stack name format
- `400 BASE_MODEL_MISMATCH` - Adapters target different base models
- `404 ADAPTER_NOT_FOUND` - One or more adapters don't exist
- `409 CONFLICT` - Stack name already exists for tenant

### List Adapter Stacks

**Endpoint:** `GET /v1/adapter-stacks`

**Required Permissions:** `AdapterView` (All roles)

**Request:**
```bash
curl -X GET "$AOS_BASE_URL/v1/adapter-stacks" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
[
  {
    "schema_version": "1.0",
    "id": "stack-xyz789",
    "tenant_id": "tenant-123",
    "name": "production-support-stack",
    "description": "Production customer support adapter stack",
    "adapter_ids": ["adapter-abc123", "adapter-def456", "adapter-ghi789"],
    "workflow_type": "parallel",
    "created_at": "2025-12-24T10:00:00Z",
    "updated_at": "2025-12-24T10:00:00Z",
    "is_active": true,
    "is_default": true,
    "version": 3,
    "lifecycle_state": "active",
    "warnings": [],
    "determinism_mode": "strict",
    "routing_determinism_mode": "deterministic"
  }
]
```

### Get Adapter Stack

**Endpoint:** `GET /v1/adapter-stacks/{id}`

**Required Permissions:** `AdapterView` (All roles)

**Request:**
```bash
curl -X GET "$AOS_BASE_URL/v1/adapter-stacks/$AOS_STACK_ID" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
{
  "schema_version": "1.0",
  "id": "stack-xyz789",
  "tenant_id": "tenant-123",
  "name": "production-support-stack",
  "description": "Production customer support adapter stack",
  "adapter_ids": ["adapter-abc123", "adapter-def456", "adapter-ghi789"],
  "workflow_type": "parallel",
  "created_at": "2025-12-24T10:00:00Z",
  "updated_at": "2025-12-24T15:30:00Z",
  "is_active": true,
  "is_default": true,
  "version": 3,
  "lifecycle_state": "active",
  "warnings": [],
  "determinism_mode": "strict",
  "routing_determinism_mode": "deterministic"
}
```

### Activate Adapter Stack

**Endpoint:** `POST /v1/adapter-stacks/{id}/activate`

**Required Permissions:** `AdapterLoad` (Operator or Admin role)

**Description:** Activates a stack for routing. Automatically promotes all adapters in the stack to at least Warm state. Performs hot-swap if a different stack was previously active.

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapter-stacks/$AOS_STACK_ID/activate" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
{
  "message": "Stack 'production-support-stack' activated for tenant 'tenant-123'",
  "stack_id": "stack-xyz789",
  "tenant_id": "tenant-123",
  "adapter_count": 3,
  "previous_stack": "stack-old-456"
}
```

**Attach Mode Validation:**

If an adapter requires dataset context but the stack doesn't provide it:

```json
{
  "error": "Adapter 'adapter-abc123' requires dataset context (dataset_version_id: dataset-456). Configure stack metadata with dataset_version_id to activate.",
  "code": "ATTACH_MODE_VIOLATION"
}
```

**Error Codes:**
- `404 NOT_FOUND` - Stack doesn't exist
- `400 ATTACH_MODE_VIOLATION` - Adapter requires dataset scope but stack lacks it
- `400 ATTACH_MODE_MISMATCH` - Stack provides wrong dataset version

### Deactivate Current Stack

**Endpoint:** `POST /v1/adapter-stacks/deactivate`

**Required Permissions:** `AdapterLoad` (Operator or Admin role)

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapter-stacks/deactivate" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
{
  "message": "Active stack deactivated",
  "tenant_id": "tenant-123",
  "previous_stack": "stack-xyz789"
}
```

### Delete Adapter Stack

**Endpoint:** `DELETE /v1/adapter-stacks/{id}`

**Required Permissions:** `AdapterRegister` (Admin or Operator role)

**Request:**
```bash
curl -X DELETE "$AOS_BASE_URL/v1/adapter-stacks/$AOS_STACK_ID" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (204 No Content)**

**Error Codes:**
- `404 NOT_FOUND` - Stack doesn't exist

---

## Streaming Inference with Adapters

### Streaming Inference Request

**Endpoint:** `POST /v1/infer/stream`

**Required Permissions:** `InferenceExecute` (All roles)

**Description:** Streams token-by-token inference output using Server-Sent Events (SSE). Compatible with OpenAI's streaming API format.

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/infer/stream" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -N \
  -d '{
    "prompt": "Explain how to reset a password in our system",
    "stack_id": "stack-xyz789",
    "max_tokens": 200,
    "temperature": 0.7,
    "top_p": 0.9,
    "stream": true
  }'
```

**Request Schema:**
```json
{
  "prompt": "string",              // User prompt (required)
  "stack_id": "string",           // Stack ID for adapter routing (optional)
  "adapter_ids": ["string"],      // Explicit adapter IDs (optional, overrides stack)
  "max_tokens": 200,              // Maximum tokens to generate (default: 100)
  "temperature": 0.7,             // Sampling temperature (default: 0.7)
  "top_p": 0.9,                   // Nucleus sampling (default: 1.0)
  "stream": true,                 // Enable streaming (default: true)
  "model": "qwen2.5-7b-instruct", // Model override (optional)
  "collection_id": "string",      // RAG collection ID (optional)
  "session_id": "string"          // Chat session ID for multi-turn (optional)
}
```

**SSE Stream Format:**

```text
event: chunk
data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1703456789,"model":"qwen2.5-7b-instruct","choices":[{"index":0,"delta":{"role":"assistant","content":"To"},"finish_reason":null}]}

event: chunk
data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1703456789,"model":"qwen2.5-7b-instruct","choices":[{"index":0,"delta":{"content":" reset"},"finish_reason":null}]}

event: chunk
data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1703456789,"model":"qwen2.5-7b-instruct","choices":[{"index":0,"delta":{"content":" your"},"finish_reason":null}]}

event: done
data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1703456789,"model":"qwen2.5-7b-instruct","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":15,"completion_tokens":42,"total_tokens":57}}
```

**JavaScript Example:**

```javascript
const eventSource = new EventSource('/v1/infer/stream', {
  headers: {
    'Authorization': `Bearer ${token}`,
    'Content-Type': 'application/json'
  },
  method: 'POST',
  body: JSON.stringify({
    prompt: 'Hello, how are you?',
    stack_id: 'stack-xyz789',
    max_tokens: 100
  })
});

eventSource.addEventListener('chunk', (event) => {
  const data = JSON.parse(event.data);
  const content = data.choices[0]?.delta?.content;
  if (content) {
    process.stdout.write(content);
  }
});

eventSource.addEventListener('done', (event) => {
  const data = JSON.parse(event.data);
  console.log('\nTokens used:', data.usage.total_tokens);
  eventSource.close();
});

eventSource.addEventListener('error', (event) => {
  console.error('Stream error:', event);
  eventSource.close();
});
```

**Error Codes:**
- `503 MODEL_NOT_READY` - Model not loaded
- `503 NO_COMPATIBLE_WORKER` - No workers available
- `429 BACKPRESSURE` - System overloaded
- `403 POLICY_HOOK_VIOLATION` - Policy check failed

---

## Adapter Lifecycle Management

### Pin Adapter (Prevent Eviction)

**Endpoint:** `POST /v1/adapters/{adapter_id}/pin`

**Required Permissions:** `AdapterLoad` (Operator or Admin role)

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapters/$AOS_ADAPTER_ID/pin" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
{
  "adapter_id": "adapter-abc123",
  "pinned": true,
  "message": "Adapter pinned successfully"
}
```

### Unpin Adapter

**Endpoint:** `DELETE /v1/adapters/{adapter_id}/pin`

**Request:**
```bash
curl -X DELETE "$AOS_BASE_URL/v1/adapters/$AOS_ADAPTER_ID/pin" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
{
  "adapter_id": "adapter-abc123",
  "pinned": false,
  "message": "Adapter unpinned successfully"
}
```

### Get Pin Status

**Endpoint:** `GET /v1/adapters/{adapter_id}/pin`

**Request:**
```bash
curl -X GET "$AOS_BASE_URL/v1/adapters/$AOS_ADAPTER_ID/pin" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
{
  "adapter_id": "adapter-abc123",
  "pinned": true,
  "pinned_at": "2025-12-24T10:30:00Z",
  "pinned_by": "admin@example.com"
}
```

---

## Adapter Repositories

### Create Adapter Repository

**Endpoint:** `POST /v1/adapter-repositories`

**Required Permissions:** `AdapterRegister` (Admin or Operator role)

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapter-repositories" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "tenant_id": "tenant-123",
    "name": "customer-support-adapters",
    "base_model_id": "qwen2.5-7b-instruct",
    "default_branch": "main",
    "description": "Repository for customer support adapters"
  }'
```

**Response (201 Created):**
```json
{
  "repo_id": "repo-abc123"
}
```

### List Adapter Repositories

**Endpoint:** `GET /v1/adapter-repositories`

**Query Parameters:**
- `base_model_id` (optional) - Filter by base model
- `archived` (optional) - Include/exclude archived repos

**Request:**
```bash
# List all repositories
curl -X GET "$AOS_BASE_URL/v1/adapter-repositories" \
  -H "Authorization: Bearer $AOS_TOKEN"

# Filter by base model
curl -X GET "$AOS_BASE_URL/v1/adapter-repositories?base_model_id=qwen2.5-7b-instruct" \
  -H "Authorization: Bearer $AOS_TOKEN"

# Include archived repositories
curl -X GET "$AOS_BASE_URL/v1/adapter-repositories?archived=true" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
[
  {
    "id": "repo-abc123",
    "tenant_id": "tenant-123",
    "name": "customer-support-adapters",
    "base_model_id": "qwen2.5-7b-instruct",
    "default_branch": "main",
    "archived": false,
    "created_by": "admin@example.com",
    "created_at": "2025-12-20T10:00:00Z",
    "description": "Repository for customer support adapters",
    "training_policy": null
  }
]
```

### Get Adapter Repository

**Endpoint:** `GET /v1/adapter-repositories/{repo_id}`

**Request:**
```bash
curl -X GET "$AOS_BASE_URL/v1/adapter-repositories/$AOS_REPO_ID" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
{
  "id": "repo-abc123",
  "tenant_id": "tenant-123",
  "name": "customer-support-adapters",
  "base_model_id": "qwen2.5-7b-instruct",
  "default_branch": "main",
  "archived": false,
  "created_by": "admin@example.com",
  "created_at": "2025-12-20T10:00:00Z",
  "description": "Repository for customer support adapters",
  "training_policy": {
    "repo_id": "repo-abc123",
    "preferred_backends": ["coreml", "mlx"],
    "coreml_allowed": true,
    "coreml_required": false,
    "autopromote_coreml": true,
    "coreml_mode": "ane",
    "repo_tier": "production",
    "auto_rollback_on_trust_regress": true,
    "created_at": "2025-12-20T10:00:00Z"
  }
}
```

### Archive Repository

**Endpoint:** `POST /v1/adapter-repositories/{repo_id}/archive`

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapter-repositories/$AOS_REPO_ID/archive" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (204 No Content)**

---

## Adapter Versions

### List Adapter Versions

**Endpoint:** `GET /v1/adapter-repositories/{repo_id}/versions`

**Query Parameters:**
- `branch` (optional) - Filter by branch name
- `state` (optional) - Filter by lifecycle state

**Request:**
```bash
# List all versions
curl -X GET "$AOS_BASE_URL/v1/adapter-repositories/$AOS_REPO_ID/versions" \
  -H "Authorization: Bearer $AOS_TOKEN"

# Filter by branch
curl -X GET "$AOS_BASE_URL/v1/adapter-repositories/$AOS_REPO_ID/versions?branch=main" \
  -H "Authorization: Bearer $AOS_TOKEN"

# Filter by state
curl -X GET "$AOS_BASE_URL/v1/adapter-repositories/$AOS_REPO_ID/versions?state=active" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
[
  {
    "id": "version-abc123",
    "repo_id": "repo-abc123",
    "tenant_id": "tenant-123",
    "version": "1.2.3",
    "branch": "main",
    "aos_path": "/var/lib/adapteros/adapters/version-abc123.aos",
    "aos_hash": "blake3:a1b2c3d4...",
    "manifest_schema_version": "1.0",
    "parent_version_id": "version-xyz456",
    "code_commit_sha": "abc123def456",
    "data_spec_hash": "blake3:e5f6g7h8...",
    "training_backend": "mlx",
    "coreml_used": false,
    "coreml_device_type": null,
    "dataset_version_ids": ["dataset-123", "dataset-456"],
    "scope_path": "support/tickets",
    "adapter_trust_state": "trusted",
    "release_state": "active",
    "metrics_snapshot_id": "metrics-789",
    "evaluation_summary": null,
    "created_at": "2025-12-24T10:30:00Z",
    "runtime_state": "warm",
    "serveable": true,
    "serveable_reason": null
  }
]
```

### Get Adapter Version

**Endpoint:** `GET /v1/adapter-versions/{version_id}`

**Request:**
```bash
curl -X GET "$AOS_BASE_URL/v1/adapter-versions/version-abc123" \
  -H "Authorization: Bearer $AOS_TOKEN"
```

**Response (200 OK):**
```json
{
  "id": "version-abc123",
  "repo_id": "repo-abc123",
  "tenant_id": "tenant-123",
  "version": "1.2.3",
  "branch": "main",
  "aos_path": "/var/lib/adapteros/adapters/version-abc123.aos",
  "aos_hash": "blake3:a1b2c3d4...",
  "manifest_schema_version": "1.0",
  "parent_version_id": "version-xyz456",
  "code_commit_sha": "abc123def456",
  "data_spec_hash": "blake3:e5f6g7h8...",
  "training_backend": "mlx",
  "coreml_used": false,
  "coreml_device_type": null,
  "dataset_version_ids": ["dataset-123", "dataset-456"],
  "scope_path": "support/tickets",
  "dataset_version_trust": [
    {
      "dataset_version_id": "dataset-123",
      "trust_at_training_time": "trusted"
    },
    {
      "dataset_version_id": "dataset-456",
      "trust_at_training_time": "trusted"
    }
  ],
  "adapter_trust_state": "trusted",
  "release_state": "active",
  "metrics_snapshot_id": "metrics-789",
  "evaluation_summary": null,
  "created_at": "2025-12-24T10:30:00Z",
  "runtime_state": "warm",
  "serveable": true,
  "serveable_reason": null
}
```

### Create Draft Version

**Endpoint:** `POST /v1/adapter-versions/draft`

**Request:**
```bash
curl -X POST "$AOS_BASE_URL/v1/adapter-versions/draft" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "repo_id": "repo-abc123",
    "branch": "main",
    "parent_version_id": "version-xyz456",
    "code_commit_sha": "def789abc012",
    "data_spec_hash": "blake3:i9j0k1l2..."
  }'
```

**Response (201 Created):**
```json
{
  "version_id": "version-new123"
}
```

---

## Complete Workflows

### Workflow 1: Register, Load, and Activate Adapter

```bash
#!/bin/bash
set -e

# 1. Register adapter
ADAPTER_ID=$(curl -sS -X POST "$AOS_BASE_URL/v1/adapters/register" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "new-support-adapter",
    "framework": "pytorch",
    "base_model": "qwen2.5-7b-instruct",
    "lora_rank": 16
  }' | jq -r '.id')

echo "Registered adapter: $ADAPTER_ID"

# 2. Load adapter
curl -sS -X POST "$AOS_BASE_URL/v1/adapters/$ADAPTER_ID/load" \
  -H "Authorization: Bearer $AOS_TOKEN"

echo "Loaded adapter: $ADAPTER_ID"

# 3. Create stack with adapter
STACK_ID=$(curl -sS -X POST "$AOS_BASE_URL/v1/adapter-stacks" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "new-support-stack",
    "adapter_ids": ["'"$ADAPTER_ID"'"],
    "workflow_type": "parallel"
  }' | jq -r '.id')

echo "Created stack: $STACK_ID"

# 4. Activate stack
curl -sS -X POST "$AOS_BASE_URL/v1/adapter-stacks/$STACK_ID/activate" \
  -H "Authorization: Bearer $AOS_TOKEN"

echo "Activated stack: $STACK_ID"

# 5. Run inference
curl -sS -X POST "$AOS_BASE_URL/v1/infer" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Test inference with new adapter",
    "stack_id": "'"$STACK_ID"'",
    "max_tokens": 50
  }' | jq '.text'
```

### Workflow 2: Import Adapter and Create Stack

```bash
#!/bin/bash
set -e

# 1. Import adapter with auto-load
ADAPTER_ID=$(curl -sS -X POST "$AOS_BASE_URL/v1/adapters/import?load=true" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -F "file=@adapter.aos" \
  | jq -r '.id')

echo "Imported and loaded adapter: $ADAPTER_ID"

# 2. Get existing adapters
EXISTING_ADAPTERS=$(curl -sS "$AOS_BASE_URL/v1/adapters" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  | jq -r '.[].id' \
  | head -n 2 \
  | jq -R . \
  | jq -s .)

# 3. Combine new adapter with existing ones
ALL_ADAPTERS=$(echo $EXISTING_ADAPTERS | jq ". + [\"$ADAPTER_ID\"]")

# 4. Create multi-adapter stack
STACK_ID=$(curl -sS -X POST "$AOS_BASE_URL/v1/adapter-stacks" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "combined-stack",
    "adapter_ids": '"$ALL_ADAPTERS"',
    "workflow_type": "parallel",
    "determinism_mode": "strict"
  }' | jq -r '.id')

echo "Created stack: $STACK_ID with adapters: $ALL_ADAPTERS"

# 5. Activate stack
curl -sS -X POST "$AOS_BASE_URL/v1/adapter-stacks/$STACK_ID/activate" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  | jq .
```

### Workflow 3: Hot-Swap Production Adapters

```bash
#!/bin/bash
set -e

OLD_ADAPTER="adapter-v1-abc123"
NEW_ADAPTER="adapter-v2-xyz789"

# 1. Verify new adapter exists and is loaded
curl -sS "$AOS_BASE_URL/v1/adapters/$NEW_ADAPTER" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  | jq '.runtime_state'

# 2. Dry-run swap validation
curl -sS -X POST "$AOS_BASE_URL/v1/adapters/swap" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "old_adapter_id": "'"$OLD_ADAPTER"'",
    "new_adapter_id": "'"$NEW_ADAPTER"'",
    "dry_run": true
  }' | jq .

# 3. Execute hot-swap
curl -sS -X POST "$AOS_BASE_URL/v1/adapters/swap" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "old_adapter_id": "'"$OLD_ADAPTER"'",
    "new_adapter_id": "'"$NEW_ADAPTER"'",
    "dry_run": false
  }' | jq .

echo "Hot-swap complete: $OLD_ADAPTER -> $NEW_ADAPTER"

# 4. Verify states
echo "Old adapter state:"
curl -sS "$AOS_BASE_URL/v1/adapters/$OLD_ADAPTER" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  | jq '.runtime_state'

echo "New adapter state:"
curl -sS "$AOS_BASE_URL/v1/adapters/$NEW_ADAPTER" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  | jq '.runtime_state'
```

### Workflow 4: Create Repository and Version Pipeline

```bash
#!/bin/bash
set -e

# 1. Create adapter repository
REPO_ID=$(curl -sS -X POST "$AOS_BASE_URL/v1/adapter-repositories" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "tenant_id": "'"$AOS_TENANT_ID"'",
    "name": "ml-experiments",
    "base_model_id": "qwen2.5-7b-instruct",
    "default_branch": "main",
    "description": "ML experimentation repository"
  }' | jq -r '.repo_id')

echo "Created repository: $REPO_ID"

# 2. Set repository training policy
curl -sS -X PUT "$AOS_BASE_URL/v1/adapter-repositories/$REPO_ID/policy" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "preferred_backends": ["mlx", "coreml"],
    "coreml_allowed": true,
    "coreml_required": false,
    "autopromote_coreml": true,
    "repo_tier": "experimental",
    "auto_rollback_on_trust_regress": true
  }' | jq .

# 3. Create draft version
VERSION_ID=$(curl -sS -X POST "$AOS_BASE_URL/v1/adapter-versions/draft" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "repo_id": "'"$REPO_ID"'",
    "branch": "main",
    "code_commit_sha": "abc123def456",
    "data_spec_hash": "blake3:a1b2c3d4..."
  }' | jq -r '.version_id')

echo "Created draft version: $VERSION_ID"

# 4. List versions
curl -sS "$AOS_BASE_URL/v1/adapter-repositories/$REPO_ID/versions" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  | jq .
```

---

## Error Handling

### Common Error Codes

| HTTP Status | Error Code | Description | Retry? |
|-------------|-----------|-------------|--------|
| 401 | UNAUTHORIZED | Invalid or missing token | No - Re-authenticate |
| 403 | FORBIDDEN | Insufficient permissions | No - Check role |
| 403 | TENANT_ACCESS_DENIED | Cross-tenant access | No - Check tenant |
| 404 | ADAPTER_NOT_FOUND | Adapter doesn't exist | No |
| 404 | STACK_NOT_FOUND | Stack doesn't exist | No |
| 400 | VALIDATION_ERROR | Invalid request format | No - Fix request |
| 400 | BASE_MODEL_MISMATCH | Incompatible base models | No - Check models |
| 409 | CONFLICT | Name already exists | No - Choose different name |
| 413 | PAYLOAD_TOO_LARGE | File exceeds 500MB | No - Reduce file size |
| 429 | BACKPRESSURE | System overloaded | Yes - Exponential backoff |
| 500 | ADAPTER_NOT_LOADABLE | Adapter can't be loaded | Maybe - Check logs |
| 503 | MODEL_NOT_READY | Model not loaded | Yes - Wait for model |
| 503 | NO_COMPATIBLE_WORKER | No workers available | Yes - Wait for workers |
| 507 | STORAGE_QUOTA_EXCEEDED | Storage limit reached | No - Free storage |

### Error Response Format

All error responses follow this schema:

```json
{
  "schema_version": "1.0",
  "error": "Detailed error message",
  "code": "ERROR_CODE",
  "details": "Additional context about the error",
  "request_id": "req-abc123"
}
```

### Retry Strategy Example

```bash
#!/bin/bash

MAX_RETRIES=3
RETRY_DELAY=2

retry_request() {
  local url=$1
  local attempt=1

  while [ $attempt -le $MAX_RETRIES ]; do
    response=$(curl -sS -w "\n%{http_code}" "$url" \
      -H "Authorization: Bearer $AOS_TOKEN")

    http_code=$(echo "$response" | tail -n1)
    body=$(echo "$response" | sed '$d')

    # Success
    if [ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]; then
      echo "$body"
      return 0
    fi

    # Don't retry client errors (4xx except 429)
    if [ "$http_code" -ge 400 ] && [ "$http_code" -lt 500 ] && [ "$http_code" -ne 429 ]; then
      echo "Client error: $body" >&2
      return 1
    fi

    # Retry server errors (5xx) and backpressure (429)
    echo "Attempt $attempt failed with HTTP $http_code, retrying..." >&2
    sleep $((RETRY_DELAY ** attempt))
    attempt=$((attempt + 1))
  done

  echo "Max retries exceeded" >&2
  return 1
}

# Usage
retry_request "$AOS_BASE_URL/v1/adapters/$AOS_ADAPTER_ID"
```

---

## Best Practices

### Adapter Naming

Use semantic names that describe the adapter's purpose:

```bash
# Good
customer-support-v1
code-review-assistant
legal-document-classifier-v2

# Bad
adapter1
test
my_adapter
```

### Stack Organization

Group related adapters in stacks:

```bash
# Production support stack
{
  "name": "production-support",
  "adapter_ids": [
    "ticket-classifier",
    "sentiment-analyzer",
    "response-generator"
  ],
  "workflow_type": "sequential"
}

# Code review stack
{
  "name": "code-review",
  "adapter_ids": [
    "bug-detector",
    "security-scanner",
    "style-checker"
  ],
  "workflow_type": "parallel"
}
```

### Lifecycle Management

Follow the recommended lifecycle progression:

```
Unloaded → Cold → Warm → Hot → Resident
```

- **Unloaded:** Not in memory
- **Cold:** Metadata loaded, weights not loaded
- **Warm:** Loaded and ready for inference
- **Hot:** Frequently used, high priority
- **Resident:** Pinned, never evicted

### Memory Management

Monitor memory pressure and adapter limits:

```bash
# Check system memory
curl -sS "$AOS_BASE_URL/v1/system/memory" \
  -H "Authorization: Bearer $AOS_TOKEN"

# List loaded adapters
curl -sS "$AOS_BASE_URL/v1/adapters" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  | jq '[.[] | select(.runtime_state != "unloaded")] | length'

# Unload unused adapters
for adapter_id in $(curl -sS "$AOS_BASE_URL/v1/adapters" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  | jq -r '.[] | select(.stats.total_activations == 0) | .id'); do

  echo "Unloading unused adapter: $adapter_id"
  curl -sS -X POST "$AOS_BASE_URL/v1/adapters/$adapter_id/unload" \
    -H "Authorization: Bearer $AOS_TOKEN"
done
```

---

## Related Documentation

- [API Reference](API_REFERENCE.md) - Complete API documentation
- [Authentication](AUTHENTICATION.md) - Auth and RBAC details
- [Access Control](ACCESS_CONTROL.md) - Permission matrix
- [Training Guide](TRAINING.md) - Training adapter workflows
- [Policies](POLICIES.md) - Policy enforcement

---

**Document Version:** 1.0.0
**Last Updated:** 2025-12-24
**Maintained By:** MLNavigator Inc.

For questions or support:
- Documentation: https://docs.adapteros.com
- Issues: GitHub Issues
- Security: security@adapteros.com
