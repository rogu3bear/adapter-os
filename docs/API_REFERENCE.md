# AdapterOS API Reference

**Version:** 1.1.0
**Last Updated:** 2025-12-20
**Copyright:** 2025 MLNavigator Inc. All rights reserved.

This document provides the complete API reference for AdapterOS, consolidating endpoint documentation, request/response formats, and integration examples.

---

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Authentication](#authentication)
4. [Core Endpoints](#core-endpoints)
   - [Public Endpoints](#public-endpoints-no-auth)
   - [Tenant Management](#tenant-management)
   - [Adapter Management](#adapter-management)
   - [Inference](#inference)
   - [Training](#training)
   - [Datasets](#datasets)
   - [Models](#models)
   - [Workers & Nodes](#workers--nodes)
   - [Metrics & Monitoring](#metrics--monitoring)
   - [Telemetry & Audit](#telemetry--audit)
   - [OpenAI Compatibility](#openai-compatibility)
   - [Replay & Determinism](#replay--determinism)
   - [RAG (Retrieval-Augmented Generation)](#retrieval-augmented-generation-rag)
   - [Chat Sessions (Extended)](#chat-sessions-extended)
   - [Policies (Extended)](#policies-extended)
   - [API Keys](#api-keys)
   - [Monitoring & Alerts](#monitoring--alerts)
   - [Plans & Orchestration](#plans--orchestration)
   - [Workspaces](#workspaces)
   - [Admin Lifecycle](#admin-lifecycle)
   - [Storage & KV Isolation](#storage--kv-isolation)
   - [Git Integration](#git-integration)
   - [Repository Management](#repository-management)
   - [Repositories API (New)](#repositories-api-new)
   - [Golden Runs & Promotion](#golden-runs--promotion)
   - [Runtime & System State](#runtime--system-state)
   - [Notifications & Tutorials](#notifications--tutorials)
   - [Dashboard Configuration](#dashboard-configuration)
   - [SSE Streaming](#sse-streaming-endpoints)
5. [LLM Interface](#llm-interface)
6. [Request/Response Formats](#requestresponse-formats)
7. [Examples](#examples)
8. [Error Handling](#error-handling)

---

## Overview

### System Architecture

AdapterOS provides a multi-tenant ML inference platform with the following components:

- **Control Plane** (port 8080): HTTP API with SQLite, JWT auth, policy enforcement
- **Worker Processes**: LoRA inference/training over Unix Domain Sockets (UDS)
- **K-Sparse Router**: Multi-adapter mixing with Q15 quantization
- **Multi-Backend**: CoreML/ANE (primary), Metal, MLX

### API Characteristics

- **Base URL**: `http://localhost:8080` (development)
- **API Version**: v1 (prefix: `/v1/`)
- **Auth**: JWT (Ed25519) with httpOnly cookies
- **Total Endpoints**: ~189 registered routes
- **Architecture**: Single-node, multi-tenant, zero network egress

```mermaid
graph TD
    A[Client] -->|HTTP/HTTPS| B[API Gateway :8080]
    B -->|JWT Auth| C[Auth Middleware]
    C -->|RBAC Check| D[Route Handler]
    D -->|UDS| E[Worker Process]
    E -->|LoRA Inference| F[Base Model]
    D -->|SQLite| G[Database]
    D -->|Policy| H[Policy Engine]
    H -->|Audit| I[Audit Log]
```

---

## Architecture

### Request Flow

```mermaid
sequenceDiagram
    participant Client
    participant API
    participant Auth
    participant Handler
    participant Worker
    participant AuditLog

    Client->>API: Request + JWT (Authorization: Bearer)
    API->>Auth: Validate Token (Ed25519)
    Auth-->>API: Claims + Role
    API->>Handler: Check Permission (RBAC)
    Handler->>Worker: Execute via UDS
    Worker-->>Handler: Result
    Handler-->>API: Response
    API->>AuditLog: Log Action (user_id, action, resource, status)
    API->>Client: JSON Response
```

### API Layers

| Layer | Responsibility |
|-------|----------------|
| **Middleware** | Security headers, rate limiting, CORS, request validation |
| **Auth** | JWT validation, session management, RBAC |
| **Handlers** | Business logic, request routing, response formatting |
| **Services** | Adapter management, training orchestration, inference |
| **Data** | SQLite persistence, KV store, telemetry |

### Key Files

| Component | File Path | Purpose |
|-----------|-----------|---------|
| Routes | `crates/adapteros-server-api/src/routes.rs` | Route definitions |
| Handlers | `crates/adapteros-server-api/src/handlers/` | Endpoint implementations |
| State | `crates/adapteros-server-api/src/state.rs` | AppState + dependency injection |
| Auth | `crates/adapteros-server-api/src/auth.rs` | JWT validation, RBAC |
| Inference | `crates/adapteros-server-api/src/inference_core.rs` | Core inference logic |

---

## Authentication

### Authentication Flow

```mermaid
sequenceDiagram
    participant Client
    participant API
    participant AuthHandler
    participant Database

    Note over Client,Database: Login Flow
    Client->>API: POST /v1/auth/login {email, password}
    API->>AuthHandler: Validate Credentials
    AuthHandler->>Database: Lookup User
    Database-->>AuthHandler: User Record
    AuthHandler->>AuthHandler: Generate JWT (Ed25519)
    AuthHandler->>Client: Set HttpOnly Cookie + Response

    Note over Client,Database: Authenticated Request
    Client->>API: GET /v1/adapters (with cookie)
    API->>AuthHandler: Extract & Validate JWT
    AuthHandler->>API: Claims (user_id, tenant_id, role)
    API->>Database: Execute Query (tenant-filtered)
    Database-->>API: Results
    API->>Client: JSON Response
```

### Token Details

**JWT Format:**
- **Algorithm**: Ed25519 (EdDSA)
- **TTL**: 8 hours (28800 seconds)
- **Claims**: `user_id`, `tenant_id`, `role`, `permissions`, `admin_tenants`
- **Cookie Name**: `auth_token` (HttpOnly, Secure in prod)

**CSRF Protection:**
- Separate `csrf_token` cookie (HttpOnly=false)
- Validated on state-changing requests

### Endpoints

#### Login

```http
POST /v1/auth/login
Content-Type: application/json

{
  "email": "admin@example.com",
  "password": "password"
}
```

**Response (200 OK):**
```json
{
  "schema_version": "1.0",
  "token": "eyJ...", // Also set in HttpOnly cookie
  "user": {
    "id": "user-123",
    "email": "admin@example.com",
    "role": "admin",
    "tenant_id": "tenant-1"
  },
  "csrf_token": "csrf-abc123"
}
```

**Error Codes:**
- `INVALID_CREDENTIALS` - Wrong email/password
- `ACCOUNT_LOCKED` - Too many failed attempts
- `ACCOUNT_DISABLED` - User deactivated
- `MFA_REQUIRED` - MFA enabled but not provided
- `DATABASE_ERROR` - System error

#### Logout

```http
POST /v1/auth/logout
Authorization: Bearer <token>
```

**Response (200 OK):**
```json
{
  "message": "Logged out successfully"
}
```

Clears `auth_token`, `refresh_token`, and `csrf_token` cookies.

#### Get Current User

```http
GET /v1/auth/me
Authorization: Bearer <token>
```

**Response (200 OK):**
```json
{
  "schema_version": "1.0",
  "id": "user-123",
  "email": "admin@example.com",
  "role": "admin",
  "tenant_id": "tenant-1",
  "permissions": ["adapter.read", "adapter.write", ...],
  "admin_tenants": ["*"]  // "*" in dev mode only
}
```

#### Refresh Token

```http
POST /v1/auth/refresh
Authorization: Bearer <refresh_token>
```

**Response (200 OK):**
```json
{
  "token": "eyJ...",
  "csrf_token": "csrf-xyz789"
}
```

#### Session Management

**List Active Sessions:**
```http
GET /v1/auth/sessions
Authorization: Bearer <token>
```

**Revoke Session:**
```http
DELETE /v1/auth/sessions/{jti}
Authorization: Bearer <token>
```

### RBAC

**5 Roles:**
- **Admin**: Full system access
- **Operator**: Day-to-day operations
- **SRE**: Infrastructure and monitoring
- **Compliance**: Audit and policy review
- **Viewer**: Read-only access

**Permission Format:** `<resource>.<action>`

**Common Permissions:**
- `adapter.read`, `adapter.write`, `adapter.register`
- `training.start`, `training.cancel`
- `policy.view`, `policy.enforce`
- `audit.view`

See [Access Control documentation](ACCESS_CONTROL.md) for complete permission matrix and RBAC details.

---

## Core Endpoints

### Public Endpoints (No Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/healthz` | Basic health check |
| GET | `/healthz/all` | All components health |
| GET | `/healthz/{component}` | Specific component health |
| GET | `/readyz` | Readiness probe |
| POST | `/v1/auth/login` | User login |
| POST | `/v1/auth/bootstrap` | Bootstrap admin (one-time) |
| POST | `/v1/auth/dev-bypass` | Dev bypass (debug builds only) |
| GET | `/v1/meta` | API metadata |
| GET | `/swagger-ui` | Swagger UI |
| GET | `/api-docs/openapi.json` | OpenAPI spec |

### Tenant Management

**List Tenants:**
```http
GET /v1/tenants
Authorization: Bearer <token>
```

**Response:**
```json
[
  {
    "schema_version": "1.0",
    "id": "tenant-1",
    "name": "acme-corp",
    "itar_flag": false,
    "created_at": "2025-11-25 10:00:00",
    "status": "active",
    "max_adapters": 50,
    "max_training_jobs": 5,
    "max_storage_gb": 100.0,
    "rate_limit_rpm": 500
  }
]
```

**Create Tenant (Admin only):**
```http
POST /v1/tenants
Authorization: Bearer <admin_token>
Content-Type: application/json

{
  "name": "startup-inc",
  "itar_flag": false
}
```

**Update Tenant:**
```http
PUT /v1/tenants/{tenant_id}
Authorization: Bearer <admin_token>
Content-Type: application/json

{
  "max_adapters": 25,
  "max_training_jobs": 3,
  "rate_limit_rpm": 300
}
```

**Tenant Operations:**
- `POST /v1/tenants/{tenant_id}/pause` - Pause tenant
- `POST /v1/tenants/{tenant_id}/archive` - Archive tenant
- `POST /v1/tenants/{tenant_id}/policies` - Assign policies
- `POST /v1/tenants/{tenant_id}/adapters` - Assign adapters
- `GET /v1/tenants/{tenant_id}/usage` - Usage statistics

### Adapter Management

**List Adapters:**
```http
GET /v1/adapters?tier=1&framework=pytorch
Authorization: Bearer <token>
```

**Response:**
```json
[
  {
    "id": "adapter-abc123",
    "name": "my-custom-adapter",
    "tenant_id": "tenant-1",
    "tier": 1,
    "framework": "pytorch",
    "load_state": "loaded",
    "lifecycle_stage": "production",
    "created_at": "2025-11-20T10:30:00Z"
  }
]
```

**Get Adapter:**
```http
GET /v1/adapters/{adapter_id}
Authorization: Bearer <token>
```

**Register Adapter:**
```http
POST /v1/adapters/register
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "customer-support-v2",
  "description": "Fine-tuned for customer support",
  "framework": "pytorch",
  "base_model": "qwen2.5-7b",
  "lora_rank": 16
}
```

**Import Adapter:**
```http
POST /v1/adapters/import
Authorization: Bearer <token>
Content-Type: multipart/form-data

file=@adapter.aos
```

**Adapter Operations:**
- `POST /v1/adapters/{adapter_id}/load` - Load adapter into memory
- `POST /v1/adapters/{adapter_id}/unload` - Unload adapter
- `DELETE /v1/adapters/{adapter_id}` - Delete adapter
- `GET /v1/adapters/{adapter_id}/activations` - Activation history
- `GET /v1/adapters/{adapter_id}/lineage` - Lineage tree
- `GET /v1/adapters/{adapter_id}/manifest` - Download manifest
- `GET /v1/adapters/{adapter_id}/health` - Health status

**Lifecycle Management:**
- `POST /v1/adapters/{adapter_id}/lifecycle/promote` - Promote stage
- `POST /v1/adapters/{adapter_id}/lifecycle/demote` - Demote stage

**Pinning:**
- `GET /v1/adapters/{adapter_id}/pin` - Get pin status
- `POST /v1/adapters/{adapter_id}/pin` - Pin adapter (prevent eviction)
- `DELETE /v1/adapters/{adapter_id}/pin` - Unpin adapter

### Inference

**Single Inference:**
```http
POST /v1/infer
Authorization: Bearer <token>
Content-Type: application/json

{
  "prompt": "What is the capital of France?",
  "adapter_ids": ["adapter-abc123"],
  "max_tokens": 100,
  "temperature": 0.7,
  "top_p": 0.9
}
```

**Response:**
```json
{
  "text": "The capital of France is Paris.",
  "tokens": 7,
  "adapters_used": ["adapter-abc123"],
  "evidence": [],
  "latency_ms": 342,
  "trace_id": "trace-xyz789",
  "cpid": "cp-20251211-abc"
}
```

**Streaming Inference (SSE):**
```http
POST /v1/infer/stream
Authorization: Bearer <token>
Content-Type: application/json

{
  "prompt": "Write a haiku about coding",
  "max_tokens": 50
}
```

**SSE Stream:**
```
event: chunk
data: {"text": "Lines", "index": 0}

event: chunk
data: {"text": " of", "index": 1}

event: done
data: {"total_tokens": 15, "latency_ms": 450}
```

**Batch Inference:**
```http
POST /v1/infer/batch
Authorization: Bearer <token>
Content-Type: application/json

{
  "prompts": [
    "Translate to French: Hello",
    "Translate to Spanish: Hello"
  ],
  "max_tokens": 20
}
```

### Training

**List Training Jobs:**
```http
GET /v1/training/jobs?status=running&limit=50
Authorization: Bearer <token>
```

**Get Training Job:**
```http
GET /v1/training/jobs/{job_id}
Authorization: Bearer <token>
```

**Response:**
```json
{
  "job_id": "job-123",
  "adapter_id": "adapter-new",
  "status": "running",
  "progress": 0.45,
  "created_at": "2025-12-11T09:00:00Z",
  "epochs_completed": 3,
  "epochs_total": 10,
  "loss": 0.023
}
```

**Start Training:**
```http
POST /v1/training/start
Authorization: Bearer <token>
Content-Type: application/json

{
  "adapter_name": "custom-adapter-v3",
  "dataset_id": "dataset-456",
  "base_model": "qwen2.5-7b",
  "epochs": 5,
  "learning_rate": 0.0001,
  "lora_rank": 16,
  "lora_alpha": 32
}
```

**Cancel Training:**
```http
POST /v1/training/jobs/{job_id}/cancel
Authorization: Bearer <token>
```

**Training Operations:**
- `GET /v1/training/jobs/{job_id}/logs` - Job logs
- `GET /v1/training/jobs/{job_id}/metrics` - Training metrics
- `GET /v1/training/jobs/{job_id}/artifacts` - Output artifacts
- `GET /v1/training/templates` - List templates
- `GET /v1/training/templates/{template_id}` - Get template

### Datasets

**Upload Dataset:**
```http
POST /v1/datasets/upload
Authorization: Bearer <token>
Content-Type: multipart/form-data

file=@training_data.jsonl
name=customer-support-qa
```

**Chunked Upload (Large Files):**

1. **Initiate:**
```http
POST /v1/datasets/chunked-upload/initiate
Authorization: Bearer <token>
Content-Type: application/json

{
  "filename": "large_dataset.jsonl",
  "total_size": 524288000,
  "chunk_size": 5242880
}
```

2. **Upload Chunks:**
```http
POST /v1/datasets/chunked-upload/{session_id}/chunk
Authorization: Bearer <token>
Content-Type: application/octet-stream

<binary chunk data>
```

3. **Complete:**
```http
POST /v1/datasets/chunked-upload/{session_id}/complete
Authorization: Bearer <token>
```

**Dataset Operations:**
- `GET /v1/datasets` - List datasets
- `GET /v1/datasets/{dataset_id}` - Get dataset
- `DELETE /v1/datasets/{dataset_id}` - Delete dataset
- `GET /v1/datasets/{dataset_id}/files` - List files
- `GET /v1/datasets/{dataset_id}/statistics` - Statistics
- `POST /v1/datasets/{dataset_id}/validate` - Validate format
- `GET /v1/datasets/{dataset_id}/preview` - Preview samples

### Models

**Get Base Model Status:**
```http
GET /v1/models/status
Authorization: Bearer <token>
```

**Response:**
```json
{
  "model_id": "qwen2.5-7b-4bit",
  "status": "ready",  // no-model, loading, ready, unloading, error, checking
  "backend": "mlx",
  "memory_mb": 4096,
  "loaded_at": "2025-12-11T08:00:00Z"
}
```

**Model Status Values:**
- `no-model` - No model loaded
- `loading` - Model loading in progress
- `ready` - Model ready for inference
- `unloading` - Model unloading
- `error` - Load/unload error
- `checking` - Health check in progress

**Model Operations:**
- `POST /v1/models/import` - Import model
- `POST /v1/models/{model_id}/load` - Load model
- `POST /v1/models/{model_id}/unload` - Unload model
- `GET /v1/models/{model_id}/status` - Get status
- `GET /v1/models/{model_id}/validate` - Validate model

**Note:** Router only allows inference when status is `ready`, otherwise returns 503 `MODEL_NOT_READY`.

### Workers & Nodes

**List Workers:**
```http
GET /v1/workers
Authorization: Bearer <token>
```

**Spawn Worker:**
```http
POST /v1/workers/spawn
Authorization: Bearer <token>
Content-Type: application/json

{
  "model_id": "qwen2.5-7b",
  "backend": "mlx"
}
```

**Worker Operations:**
- `GET /v1/workers/{worker_id}/logs` - Worker logs
- `GET /v1/workers/{worker_id}/crashes` - Crash reports
- `POST /v1/workers/{worker_id}/debug` - Debug session
- `POST /v1/workers/{worker_id}/troubleshoot` - Troubleshoot
- `POST /v1/workers/{worker_id}/stop` - Stop worker

**Node Operations:**
- `GET /v1/nodes` - List nodes
- `POST /v1/nodes/register` - Register node
- `POST /v1/nodes/{node_id}/ping` - Test connection
- `POST /v1/nodes/{node_id}/offline` - Mark offline
- `DELETE /v1/nodes/{node_id}` - Evict node
- `GET /v1/nodes/{node_id}/details` - Node details

### Metrics & Monitoring

**Prometheus Metrics:**
```http
GET /v1/metrics
Authorization: Bearer <metrics_token>
```

**Metrics Endpoints:**
- `GET /v1/metrics/quality` - Quality metrics
- `GET /v1/metrics/adapters` - Adapter metrics
- `GET /v1/metrics/system` - System metrics
- `GET /v1/metrics/snapshot` - Point-in-time snapshot
- `GET /v1/metrics/series` - Time series data

**Key Metrics:**
- `adapteros_model_load_success_total` - Successful model loads
- `adapteros_model_load_failure_total` - Failed model loads
- `adapteros_model_loaded` - Current loaded state (0 or 1)
- `adapteros_model_unload_success_total` - Successful unloads
- `adapteros_model_unload_failure_total` - Failed unloads

**Monitoring:**
- `GET /v1/monitoring/rules` - List monitoring rules
- `POST /v1/monitoring/rules` - Create rule
- `GET /v1/monitoring/alerts` - List alerts
- `POST /v1/monitoring/alerts/{alert_id}/acknowledge` - Ack alert
- `GET /v1/monitoring/dashboards` - List dashboards

### Telemetry & Audit

**Query Audit Logs:**
```http
GET /v1/audit/logs?action=adapter.load&limit=100&offset=0
Authorization: Bearer <token>
```

**Response:**
```json
[
  {
    "id": "audit-123",
    "timestamp": "2025-12-11T10:30:00Z",
    "user_id": "user-456",
    "action": "adapter.load",
    "resource_type": "adapter",
    "resource_id": "adapter-abc",
    "status": "success",
    "tenant_id": "tenant-1"
  }
]
```

**Telemetry:**
- `GET /v1/telemetry/bundles` - List telemetry bundles
- `GET /v1/telemetry/bundles/{bundle_id}/export` - Export bundle
- `POST /v1/telemetry/bundles/{bundle_id}/verify` - Verify signature
- `POST /v1/telemetry/bundles/purge` - Purge old bundles

**Traces & Logs:**
- `GET /v1/traces/search` - Search traces
- `GET /v1/traces/{trace_id}` - Get trace
- `GET /v1/logs/query` - Query logs
- `GET /v1/logs/stream` - Stream logs (SSE)

### OpenAI Compatibility

**OpenAI-Compatible Chat Completions:**
```http
POST /v1/chat/completions
Authorization: Bearer <token>
Content-Type: application/json

{
  "model": "qwen2.5-7b",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello!"}
  ],
  "temperature": 0.7,
  "max_tokens": 100,
  "stream": false
}
```

**Response:**
```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "created": 1702345678,
  "model": "qwen2.5-7b",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello! How can I assist you today?"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 15,
    "completion_tokens": 10,
    "total_tokens": 25
  }
}
```

**Note:** This endpoint provides OpenAI API compatibility for tools like Cursor IDE and other OpenAI clients. Supports both streaming and non-streaming modes.

### Replay & Determinism

**Check Replay Availability:**
```http
GET /v1/replay/check/{inference_id}
Authorization: Bearer <token>
```

**Response:**
```json
{
  "available": true,
  "inference_id": "inf-abc123",
  "manifest_hash": "blake3-xyz789",
  "router_seed": "seed-123",
  "created_at": "2025-12-11T10:30:00Z"
}
```

**Execute Replay:**
```http
POST /v1/replay
Authorization: Bearer <token>
Content-Type: application/json

{
  "inference_id": "inf-abc123",
  "verify_determinism": true
}
```

**Response:**
```json
{
  "text": "Replayed response text",
  "tokens": 42,
  "matches_original": true,
  "trace_id": "trace-replay-xyz",
  "latency_ms": 156,
  "verification": {
    "output_hash_match": true,
    "token_count_match": true,
    "adapter_sequence_match": true
  }
}
```

**Replay Sessions:**
- `GET /v1/replay/sessions` - List replay sessions
- `POST /v1/replay/sessions` - Create replay session
- `GET /v1/replay/sessions/{id}` - Get replay session
- `POST /v1/replay/sessions/{id}/verify` - Verify replay
- `POST /v1/replay/sessions/{id}/execute` - Execute replay
- `GET /v1/replay/history/{inference_id}` - Get replay history

**Determinism Status:**
```http
GET /v1/diagnostics/determinism
Authorization: Bearer <token>
```

**Response:**
```json
{
  "determinism_enabled": true,
  "seed_source": "hkdf_sha256",
  "router_deterministic": true,
  "replay_success_rate": 0.998,
  "total_replays": 15234,
  "failed_replays": 31
}
```

**Quarantine Status:**
```http
GET /v1/diagnostics/quarantine
Authorization: Bearer <token>
```

**Response:**
```json
{
  "quarantined_adapters": [
    {
      "adapter_id": "adapter-123",
      "reason": "non_deterministic_output",
      "quarantined_at": "2025-12-11T09:00:00Z",
      "replay_failures": 5
    }
  ],
  "total_quarantined": 1
}
```

### Retrieval-Augmented Generation (RAG)

**Note:** RAG operations are primarily handled internally by `InferenceCore` but evidence can be queried.

**List Evidence:**
```http
GET /v1/evidence?inference_id=inf-123&limit=50
Authorization: Bearer <token>
```

**Create Evidence:**
```http
POST /v1/evidence
Authorization: Bearer <token>
Content-Type: application/json

{
  "source_type": "document",
  "source_id": "doc-456",
  "span_text": "Relevant context from document",
  "relevance_score": 0.92,
  "metadata": {
    "chunk_id": "chunk-789",
    "position": {"start": 100, "end": 150}
  }
}
```

**Get Dataset Evidence:**
```http
GET /v1/datasets/{dataset_id}/evidence
Authorization: Bearer <token>
```

**Get Adapter Evidence:**
```http
GET /v1/adapters/{adapter_id}/evidence
Authorization: Bearer <token>
```

### Chat Sessions (Extended)

**Advanced Chat Operations:**

**Search Sessions:**
```http
GET /v1/chat/sessions/search?q=machine+learning&limit=20
Authorization: Bearer <token>
```

**Archived Sessions:**
```http
GET /v1/chat/sessions/archived
Authorization: Bearer <token>
```

**Deleted Sessions (Trash):**
```http
GET /v1/chat/sessions/trash
Authorization: Bearer <token>
```

**Shared Sessions:**
```http
GET /v1/chat/sessions/shared-with-me
Authorization: Bearer <token>
```

**Update Session:**
```http
PUT /v1/chat/sessions/{session_id}
Authorization: Bearer <token>
Content-Type: application/json

{
  "title": "Updated Session Title",
  "description": "Updated description"
}
```

**Fork Session:**
```http
POST /v1/chat/sessions/{session_id}/fork
Authorization: Bearer <token>
Content-Type: application/json

{
  "title": "Forked Session",
  "fork_from_message_id": "msg-123"
}
```

**Archive/Restore:**
- `POST /v1/chat/sessions/{session_id}/archive` - Archive session
- `POST /v1/chat/sessions/{session_id}/restore` - Restore session
- `DELETE /v1/chat/sessions/{session_id}/permanent` - Permanently delete

**Session Sharing:**
- `GET /v1/chat/sessions/{session_id}/shares` - List shares
- `POST /v1/chat/sessions/{session_id}/shares` - Share session
- `DELETE /v1/chat/sessions/{session_id}/shares/{share_id}` - Revoke share

**Tags & Categories:**
- `GET /v1/chat/tags` - List tags
- `POST /v1/chat/tags` - Create tag
- `PUT /v1/chat/tags/{tag_id}` - Update tag
- `DELETE /v1/chat/tags/{tag_id}` - Delete tag
- `GET /v1/chat/categories` - List categories
- `POST /v1/chat/categories` - Create category
- `GET /v1/chat/sessions/{session_id}/tags` - Get session tags
- `POST /v1/chat/sessions/{session_id}/tags` - Assign tags
- `PUT /v1/chat/sessions/{session_id}/category` - Set category

**Chat Provenance:**
```http
GET /v1/chat/sessions/{session_id}/provenance
Authorization: Bearer <token>
```

**Response:**
```json
{
  "session_id": "session-123",
  "adapters_used": ["adapter-abc", "adapter-def"],
  "models_used": ["qwen2.5-7b"],
  "evidence_sources": [
    {
      "doc_id": "doc-456",
      "message_id": "msg-789",
      "relevance": 0.95
    }
  ],
  "policy_decisions": ["egress_blocked", "determinism_enforced"]
}
```

### Policies (Extended)

**List Policies:**
```http
GET /v1/policies
Authorization: Bearer <token>
```

**Get Policy:**
```http
GET /v1/policies/{cpid}
Authorization: Bearer <token>
```

**Validate Policy:**
```http
POST /v1/policies/validate
Authorization: Bearer <token>
Content-Type: application/json

{
  "policy_type": "egress",
  "configuration": {
    "allow_network": false,
    "allow_filesystem": true
  }
}
```

**Apply Policy:**
```http
POST /v1/policies/apply
Authorization: Bearer <token>
Content-Type: application/json

{
  "cpid": "cp-123",
  "policy_ids": ["policy-egress", "policy-determinism"],
  "enforcement_level": "strict"
}
```

**Sign Policy:**
```http
POST /v1/policies/{cpid}/sign
Authorization: Bearer <token>
```

**Verify Policy Signature:**
```http
GET /v1/policies/{cpid}/verify
Authorization: Bearer <token>
```

**Compare Policy Versions:**
```http
POST /v1/policies/compare
Authorization: Bearer <token>
Content-Type: application/json

{
  "policy_a": "cp-123",
  "policy_b": "cp-456"
}
```

**Export Policy:**
```http
GET /v1/policies/{cpid}/export
Authorization: Bearer <token>
```

**Policy Assignments:**
- `POST /v1/policies/assign` - Assign policy to tenant
- `GET /v1/policies/assignments` - List assignments
- `GET /v1/policies/violations` - List violations

**Policy Decisions:**
```http
GET /v1/tenants/{tenant_id}/policy-decisions?limit=100
Authorization: Bearer <token>
```

**Verify Policy Audit Chain:**
```http
POST /v1/tenants/{tenant_id}/policy-audit/verify
Authorization: Bearer <token>
```

### API Keys

**List API Keys:**
```http
GET /v1/api-keys
Authorization: Bearer <token>
```

**Response:**
```json
[
  {
    "id": "key-123",
    "name": "Production API Key",
    "prefix": "aos_pk_",
    "created_at": "2025-12-01T10:00:00Z",
    "last_used_at": "2025-12-11T15:30:00Z",
    "expires_at": "2026-12-01T10:00:00Z",
    "scopes": ["inference", "adapters.read"]
  }
]
```

**Create API Key:**
```http
POST /v1/api-keys
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "New API Key",
  "scopes": ["inference", "adapters.read"],
  "expires_in_days": 365
}
```

**Response:**
```json
{
  "id": "key-456",
  "name": "New API Key",
  "key": "aos_pk_abc123def456...",
  "prefix": "aos_pk_",
  "created_at": "2025-12-11T16:00:00Z",
  "expires_at": "2026-12-11T16:00:00Z",
  "scopes": ["inference", "adapters.read"]
}
```

**Warning:** The full key is only shown once during creation. Store it securely.

**Revoke API Key:**
```http
DELETE /v1/api-keys/{id}
Authorization: Bearer <token>
```

### Monitoring & Alerts

**List Monitoring Rules:**
```http
GET /v1/monitoring/rules
Authorization: Bearer <token>
```

**Create Monitoring Rule:**
```http
POST /v1/monitoring/rules
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "High Latency Alert",
  "metric": "inference_latency_ms",
  "condition": "greater_than",
  "threshold": 1000,
  "window_minutes": 5,
  "severity": "warning"
}
```

**Update Monitoring Rule:**
```http
PUT /v1/monitoring/rules/{rule_id}
Authorization: Bearer <token>
Content-Type: application/json

{
  "threshold": 2000,
  "severity": "critical"
}
```

**Delete Monitoring Rule:**
```http
DELETE /v1/monitoring/rules/{rule_id}
Authorization: Bearer <token>
```

**List Alerts:**
```http
GET /v1/monitoring/alerts?status=active&severity=critical
Authorization: Bearer <token>
```

**Acknowledge Alert:**
```http
POST /v1/monitoring/alerts/{alert_id}/acknowledge
Authorization: Bearer <token>
Content-Type: application/json

{
  "acknowledged_by": "admin@example.com",
  "note": "Investigating issue"
}
```

**Resolve Alert:**
```http
POST /v1/monitoring/alerts/{alert_id}/resolve
Authorization: Bearer <token>
Content-Type: application/json

{
  "resolution": "Increased worker capacity",
  "resolved_by": "admin@example.com"
}
```

**List Process Anomalies:**
```http
GET /v1/monitoring/anomalies
Authorization: Bearer <token>
```

**List Health Metrics:**
```http
GET /v1/monitoring/health-metrics
Authorization: Bearer <token>
```

### Plans & Orchestration

**List Plans:**
```http
GET /v1/plans
Authorization: Bearer <token>
```

**Build Plan:**
```http
POST /v1/plans/build
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "Production Deployment",
  "adapters": ["adapter-abc", "adapter-def"],
  "target_tier": 1
}
```

**Get Plan Details:**
```http
GET /v1/plans/{plan_id}/details
Authorization: Bearer <token>
```

**Rebuild Plan:**
```http
POST /v1/plans/{plan_id}/rebuild
Authorization: Bearer <token>
```

**Compare Plans:**
```http
POST /v1/plans/compare
Authorization: Bearer <token>
Content-Type: application/json

{
  "plan_a": "plan-123",
  "plan_b": "plan-456"
}
```

**Export Plan Manifest:**
```http
GET /v1/plans/{plan_id}/manifest
Authorization: Bearer <token>
```

**CPID Promotion:**
- `POST /v1/cp/promote` - Promote CPID
- `GET /v1/cp/promotion-gates/{cpid}` - Get promotion gates
- `POST /v1/cp/rollback` - Rollback CPID
- `POST /v1/cp/promote/dry-run` - Dry-run promotion
- `GET /v1/cp/promotions` - Promotion history

### Workspaces

**List Workspaces:**
```http
GET /v1/workspaces
Authorization: Bearer <token>
```

**List User Workspaces:**
```http
GET /v1/workspaces/user
Authorization: Bearer <token>
```

**Create Workspace:**
```http
POST /v1/workspaces
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "ML Research Team",
  "description": "Workspace for ML research projects",
  "visibility": "private"
}
```

**Get Workspace:**
```http
GET /v1/workspaces/{workspace_id}
Authorization: Bearer <token>
```

**Update Workspace:**
```http
PUT /v1/workspaces/{workspace_id}
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "Updated Workspace Name",
  "description": "Updated description"
}
```

**Delete Workspace:**
```http
DELETE /v1/workspaces/{workspace_id}
Authorization: Bearer <token>
```

**Workspace Members:**
- `GET /v1/workspaces/{workspace_id}/members` - List members
- `POST /v1/workspaces/{workspace_id}/members` - Add member
- `PUT /v1/workspaces/{workspace_id}/members/{user_id}` - Update member role
- `DELETE /v1/workspaces/{workspace_id}/members/{user_id}` - Remove member

**Workspace Resources:**
- `GET /v1/workspaces/{workspace_id}/resources` - List resources
- `POST /v1/workspaces/{workspace_id}/resources` - Share resource
- `DELETE /v1/workspaces/{workspace_id}/resources/{resource_id}` - Unshare resource

### Admin Lifecycle

**Request Shutdown:**
```http
POST /admin/lifecycle/request-shutdown
Authorization: Bearer <admin_token>
Content-Type: application/json

{
  "reason": "Planned maintenance",
  "delay_seconds": 300
}
```

**Request Maintenance Mode:**
```http
POST /admin/lifecycle/request-maintenance
Authorization: Bearer <admin_token>
```

**Safe Restart:**
```http
POST /admin/lifecycle/safe-restart
Authorization: Bearer <admin_token>
Content-Type: application/json

{
  "drain_timeout_seconds": 600
}
```

### Storage & KV Isolation

**Get Storage Mode:**
```http
GET /v1/storage/mode
Authorization: Bearer <token>
```

**Response:**
```json
{
  "primary": "sqlite",
  "kv_backend": "sqlite",
  "kv_isolation_enabled": true
}
```

**Get Storage Stats:**
```http
GET /v1/storage/stats
Authorization: Bearer <token>
```

**Response:**
```json
{
  "database_size_mb": 1024.5,
  "table_counts": {
    "adapters": 150,
    "training_jobs": 42,
    "chat_sessions": 1024
  },
  "kv_counts": {
    "chat_sessions_kv": 1024,
    "users_kv": 56
  }
}
```

**KV Isolation Health:**
```http
GET /v1/kv-isolation/health
Authorization: Bearer <token>
```

**Trigger KV Isolation Scan:**
```http
POST /v1/kv-isolation/scan
Authorization: Bearer <token>
```

### Git Integration

**Git Status:**
```http
GET /v1/git/status?repo_path=/path/to/repo
Authorization: Bearer <token>
```

**Response:**
```json
{
  "current_branch": "main",
  "status": "clean",
  "staged_files": [],
  "modified_files": [],
  "untracked_files": [],
  "ahead": 0,
  "behind": 0
}
```

**List Git Branches:**
```http
GET /v1/git/branches?repo_path=/path/to/repo
Authorization: Bearer <token>
```

**Response:**
```json
{
  "branches": [
    {
      "name": "main",
      "is_current": true,
      "last_commit": "abc123",
      "last_commit_message": "Fix bug in router"
    },
    {
      "name": "feature/new-adapter",
      "is_current": false,
      "last_commit": "def456",
      "last_commit_message": "Add new adapter"
    }
  ]
}
```

**Start Git Session:**
```http
POST /v1/git/sessions
Authorization: Bearer <token>
Content-Type: application/json

{
  "repo_path": "/path/to/repo",
  "action": "commit",
  "message": "Implement new feature"
}
```

**Response:**
```json
{
  "session_id": "git-session-123",
  "status": "active",
  "repo_path": "/path/to/repo",
  "created_at": "2025-12-11T16:00:00Z"
}
```

**End Git Session:**
```http
POST /v1/git/sessions/{session_id}/end
Authorization: Bearer <token>
Content-Type: application/json

{
  "action": "commit",
  "commit_message": "Final commit"
}
```

**File Changes Stream (SSE):**
```http
GET /v1/git/file-changes?repo_path=/path/to/repo
Authorization: Bearer <token>
```

**SSE Events:**
```
event: file_modified
data: {"path": "src/main.rs", "change_type": "modified"}

event: file_created
data: {"path": "src/new.rs", "change_type": "created"}
```

### Repository Management

**List Repositories (Code Intelligence):**
```http
GET /v1/code/repositories
Authorization: Bearer <token>
```

**Register Repository:**
```http
POST /v1/code/register-repo
Authorization: Bearer <token>
Content-Type: application/json

{
  "path": "/path/to/repo",
  "name": "my-project",
  "language": "rust"
}
```

**Scan Repository:**
```http
POST /v1/code/scan
Authorization: Bearer <token>
Content-Type: application/json

{
  "repo_id": "repo-123",
  "scan_type": "full"
}
```

**Get Scan Status:**
```http
GET /v1/code/scan/{job_id}
Authorization: Bearer <token>
```

**Response:**
```json
{
  "job_id": "scan-456",
  "status": "in_progress",
  "progress": 0.65,
  "files_scanned": 1234,
  "total_files": 1897,
  "started_at": "2025-12-11T15:00:00Z"
}
```

**Get Repository:**
```http
GET /v1/code/repositories/{repo_id}
Authorization: Bearer <token>
```

**Create Commit Delta:**
```http
POST /v1/code/commit-delta
Authorization: Bearer <token>
Content-Type: application/json

{
  "repo_id": "repo-123",
  "from_commit": "abc123",
  "to_commit": "def456"
}
```

### Repositories API (New)

**List Repos:**
```http
GET /v1/repos
Authorization: Bearer <token>
```

**Get Repo:**
```http
GET /v1/repos/{repo_id}
Authorization: Bearer <token>
```

**Create Repo:**
```http
POST /v1/repos
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "ml-experiments",
  "description": "Machine learning experiments repository",
  "visibility": "private"
}
```

**Update Repo:**
```http
PUT /v1/repos/{repo_id}
Authorization: Bearer <token>
Content-Type: application/json

{
  "description": "Updated description",
  "visibility": "public"
}
```

### Golden Runs & Promotion

**List Golden Runs:**
```http
GET /v1/golden/runs?adapter_id=adapter-123
Authorization: Bearer <token>
```

**Get Golden Run:**
```http
GET /v1/golden/runs/{run_id}
Authorization: Bearer <token>
```

**Compare Golden Run:**
```http
POST /v1/golden/compare
Authorization: Bearer <token>
Content-Type: application/json

{
  "golden_run_id": "golden-123",
  "test_run_id": "test-456"
}
```

**Response:**
```json
{
  "match": true,
  "output_similarity": 1.0,
  "token_count_match": true,
  "latency_delta_ms": 5,
  "differences": []
}
```

**Request Promotion:**
```http
POST /v1/promotion/request
Authorization: Bearer <token>
Content-Type: application/json

{
  "adapter_id": "adapter-123",
  "from_stage": "staging",
  "to_stage": "production",
  "justification": "Passed all tests"
}
```

**Get Promotion Status:**
```http
GET /v1/promotion/status/{promotion_id}
Authorization: Bearer <token>
```

**Approve/Reject Promotion:**
```http
POST /v1/promotion/{promotion_id}/approve
Authorization: Bearer <token>
Content-Type: application/json

{
  "approved": true,
  "reviewer_notes": "Looks good, approved for production"
}
```

**Rollback Promotion:**
```http
POST /v1/promotion/{promotion_id}/rollback
Authorization: Bearer <token>
Content-Type: application/json

{
  "reason": "Performance regression detected"
}
```

**Get Gate Status:**
```http
GET /v1/promotion/{promotion_id}/gates
Authorization: Bearer <token>
```

**Response:**
```json
{
  "gates": [
    {
      "name": "tests_passing",
      "status": "passed",
      "checked_at": "2025-12-11T14:00:00Z"
    },
    {
      "name": "security_review",
      "status": "pending",
      "checked_at": null
    }
  ],
  "all_passed": false
}
```

### Runtime & System State

**Get Current Session:**
```http
GET /v1/runtime/session
Authorization: Bearer <token>
```

**List Runtime Sessions:**
```http
GET /v1/runtime/sessions?status=active&limit=50
Authorization: Bearer <token>
```

**Get System State (Ground Truth):**
```http
GET /v1/system/state
Authorization: Bearer <token>
```

**Response:**
```json
{
  "schema_version": "1.0",
  "timestamp": "2025-12-11T16:30:00Z",
  "origin": "control_plane",
  "nodes": [
    {
      "node_id": "node-1",
      "status": "healthy",
      "workers": 4,
      "memory_mb": 16384
    }
  ],
  "services": [
    {
      "name": "adapteros-server",
      "status": "running",
      "health": "healthy",
      "uptime_seconds": 86400
    }
  ],
  "tenants": [
    {
      "tenant_id": "tenant-1",
      "active_adapters": 12,
      "active_stacks": 3
    }
  ],
  "memory": {
    "total_mb": 32768,
    "used_mb": 18432,
    "pressure_level": "normal"
  }
}
```

**Get System Overview:**
```http
GET /v1/system/overview
Authorization: Bearer <token>
```

**Get Pilot Status:**
```http
GET /v1/system/pilot-status
Authorization: Bearer <token>
```

**Get UMA Memory:**
```http
GET /v1/system/memory
Authorization: Bearer <token>
```

**Get UMA Memory Breakdown:**
```http
GET /v1/system/memory/detail
Authorization: Bearer <token>
```

**Get Adapter Memory Usage:**
```http
GET /v1/system/memory/adapters
Authorization: Bearer <token>
```

### Notifications & Tutorials

**List Notifications:**
```http
GET /v1/notifications?read=false&limit=20
Authorization: Bearer <token>
```

**Get Notification Summary:**
```http
GET /v1/notifications/summary
Authorization: Bearer <token>
```

**Response:**
```json
{
  "total": 15,
  "unread": 7,
  "by_severity": {
    "info": 10,
    "warning": 4,
    "critical": 1
  }
}
```

**Mark Notification Read:**
```http
POST /v1/notifications/{notification_id}/read
Authorization: Bearer <token>
```

**Mark All Notifications Read:**
```http
POST /v1/notifications/read-all
Authorization: Bearer <token>
```

**List Tutorials:**
```http
GET /v1/tutorials
Authorization: Bearer <token>
```

**Mark Tutorial Completed:**
```http
POST /v1/tutorials/{tutorial_id}/complete
Authorization: Bearer <token>
```

**Mark Tutorial Dismissed:**
```http
POST /v1/tutorials/{tutorial_id}/dismiss
Authorization: Bearer <token>
```

### Dashboard Configuration

**Get Dashboard Config:**
```http
GET /v1/dashboard/config
Authorization: Bearer <token>
```

**Update Dashboard Config:**
```http
PUT /v1/dashboard/config
Authorization: Bearer <token>
Content-Type: application/json

{
  "widgets": [
    {
      "id": "widget-1",
      "type": "adapter_metrics",
      "position": {"x": 0, "y": 0},
      "size": {"width": 6, "height": 4}
    }
  ]
}
```

**Reset Dashboard Config:**
```http
POST /v1/dashboard/config/reset
Authorization: Bearer <token>
```

### SSE Streaming Endpoints

All streaming endpoints use Server-Sent Events (SSE) and require authentication.

| Path | Description |
|------|-------------|
| `/v1/streams/training` | Training events |
| `/v1/streams/discovery` | Discovery events |
| `/v1/streams/contacts` | Contacts events |
| `/v1/streams/file-changes` | File change notifications |
| `/v1/stream/metrics` | System metrics stream |
| `/v1/stream/telemetry` | Telemetry events |
| `/v1/stream/adapters` | Adapter state changes |

**Example SSE Connection:**
```javascript
const eventSource = new EventSource('/v1/stream/metrics', {
  headers: {
    'Authorization': `Bearer ${token}`
  }
});

eventSource.onmessage = (event) => {
  const data = JSON.parse(event.data);
  console.log('Metric:', data);
};
```

---

## LLM Interface

### Overview

The LLM interface specification defines how the base language model interacts with the AdapterOS runtime. This enables:
- Function calling for retrieval and computation
- Signal-based adapter activation
- Evidence-grounded responses
- Policy enforcement

### Core Interface

```typescript
interface LLMRuntime {
  // Inference entry point
  async generate(request: GenerateRequest): Promise<GenerateResponse>;

  // Streaming variant
  async generateStream(request: GenerateRequest): AsyncIterator<StreamChunk>;

  // Adapter management
  async loadAdapter(adapterId: string): Promise<AdapterHandle>;
  async unloadAdapter(adapterId: string): Promise<void>;
}

interface GenerateRequest {
  prompt: string;
  tenantId: string;
  cpid: string;  // Control Plane ID
  maxTokens: number;
  temperature: number;
  topP: number;

  // Policy constraints
  policyContext: PolicyContext;
  requireEvidence: boolean;
  minEvidenceSpans: number;
}

interface GenerateResponse {
  text: string;
  tokens: number;
  adaptersUsed: AdapterActivation[];
  evidence: EvidenceSpan[];
  latencyMs: number;
  traceId: string;
}
```

### Function Catalog

#### Retrieval Functions

**retrieve_evidence** - Semantic search over knowledge base:
```typescript
async function retrieve_evidence(params: {
  query: string;
  topK: number;
  filters?: {
    docType?: string[];
    dateRange?: [Date, Date];
  };
}): Promise<{
  spans: EvidenceSpan[];
  retrievalTimeMs: number;
}>;
```

**retrieve_by_id** - Direct document retrieval:
```typescript
async function retrieve_by_id(params: {
  docId: string;
  revision?: string;
  spanIds?: string[];
}): Promise<{
  document: Document;
  spans: EvidenceSpan[];
}>;
```

**search_code** - Code repository search:
```typescript
async function search_code(params: {
  query: string;
  language?: string[];
  repoId?: string;
  maxResults: number;
}): Promise<{
  results: CodeMatch[];
}>;
```

#### Computation Functions

**calculate** - Mathematical computations:
```typescript
async function calculate(params: {
  expression: string;
  units?: {
    input: Record<string, string>;
    output: string;
  };
  precision?: number;
}): Promise<{
  result: number;
  units: string;
  steps?: string[];
}>;
```

**convert_units** - Unit conversion:
```typescript
async function convert_units(params: {
  value: number;
  fromUnit: string;
  toUnit: string;
}): Promise<{
  value: number;
}>;
```

#### State Functions

**store_context** - Store session context:
```typescript
async function store_context(params: {
  key: string;
  value: any;
  scope: 'session' | 'turn' | 'persistent';
  ttl?: number;
}): Promise<{
  stored: boolean;
  contextId: string;
}>;
```

**retrieve_context** - Retrieve stored context:
```typescript
async function retrieve_context(params: {
  key: string;
  scope: 'session' | 'turn' | 'persistent';
}): Promise<{
  value: any;
  timestamp: Date;
}>;
```

#### Policy Functions

**check_policy** - Query policy constraints:
```typescript
async function check_policy(params: {
  action: string;
  resource?: string;
  context?: Record<string, any>;
}): Promise<{
  allowed: boolean;
  reason?: string;
  alternatives?: string[];
}>;
```

**should_refuse** - Determine if query should be refused:
```typescript
async function should_refuse(params: {
  query: string;
  evidence: EvidenceSpan[];
  confidence: number;
}): Promise<{
  refuse: boolean;
  reason?: string;
  suggestedQuestions?: string[];
}>;
```

### Signal Protocol

Signals are lightweight notifications from LLM to runtime:

```typescript
enum SignalType {
  // Adapter routing
  ADAPTER_REQUEST = 'adapter.request',
  ADAPTER_ACTIVATE = 'adapter.activate',

  // Evidence
  EVIDENCE_REQUIRED = 'evidence.required',
  EVIDENCE_CITE = 'evidence.cite',
  EVIDENCE_INSUFFICIENT = 'evidence.insufficient',

  // Policy
  POLICY_CHECK = 'policy.check',
  REFUSAL_INTENT = 'refusal.intent',

  // State
  CONTEXT_SAVE = 'context.save',
  CHECKPOINT_REQUEST = 'checkpoint.request',
}

interface Signal {
  type: SignalType;
  timestamp: Date;
  payload: Record<string, any>;
  priority: 'low' | 'normal' | 'high' | 'critical';
}
```

**Example Usage:**
```typescript
// Request specific adapters
emit_signal({
  type: SignalType.ADAPTER_REQUEST,
  payload: {
    requestedAdapters: ['aviation_maintenance', 'boeing_737'],
    reason: 'maintenance_procedure_query',
  },
  priority: 'normal',
  timestamp: new Date()
});

// Cite evidence
emit_signal({
  type: SignalType.EVIDENCE_CITE,
  payload: {
    spanId: 'doc_123:span_456',
    textPosition: { start: 150, end: 175 },
    citationType: 'direct'
  },
  priority: 'normal',
  timestamp: new Date()
});
```

### Determinism & Constraints

**Key Principles:**
1. **Determinism First**: Same inputs + CPID → Same outputs
2. **Zero Egress**: No network access during inference
3. **Policy Enforcement**: All operations subject to tenant policies
4. **Evidence Traceability**: Every claim must cite sources

**Security Constraints:**
- No network operations (`fetch`, `WebSocket`, etc.)
- Tenant data isolation enforced
- All tool executions logged
- BLAKE3 hashing for integrity

---

## Request/Response Formats

### Standard Response Format

All API responses follow this structure:

```json
{
  "schema_version": "1.0",
  "data": { ... },
  "metadata": {
    "request_id": "req-abc123",
    "timestamp": "2025-12-11T10:30:00Z"
  }
}
```

### Error Response Format

```json
{
  "schema_version": "1.0",
  "error": "Detailed error message",
  "code": "ERROR_CODE",
  "details": "Additional context",
  "request_id": "req-abc123"
}
```

### Common Error Codes

#### Authentication Errors (401)
- `INVALID_CREDENTIALS` - Wrong username/password
- `MFA_REQUIRED` - MFA needed but not provided
- `INVALID_MFA_CODE` - Incorrect MFA code
- `ACCOUNT_LOCKED` - Too many failed attempts
- `ACCOUNT_DISABLED` - Account deactivated
- `SESSION_EXPIRED` - JWT token expired
- `UNAUTHORIZED` - Not authenticated
- `TOKEN_REVOKED` - Token has been revoked
- `INVALID_TOKEN` - Malformed or invalid token
- `TOKEN_EXPIRED` - Token lifetime exceeded

#### Authorization Errors (403)
- `FORBIDDEN` - Insufficient permissions
- `NO_TENANT_ACCESS` - No access to tenant
- `TENANT_ACCESS_DENIED` - Cross-tenant access denied
- `INSUFFICIENT_ROLE` - User role insufficient for operation
- `PERMISSION_DENIED` - Specific permission not granted
- `ADMIN_ONLY` - Operation requires admin role
- `WORKSPACE_ACCESS_DENIED` - No access to workspace

#### Validation Errors (400)
- `VALIDATION_ERROR` - Request validation failed
- `WEAK_PASSWORD` - Password doesn't meet requirements
- `INVALID_MFA_CODE` - MFA code format invalid
- `INVALID_REQUEST_BODY` - Malformed JSON or missing fields
- `INVALID_PARAMETER` - Query parameter invalid
- `MISSING_REQUIRED_FIELD` - Required field not provided
- `SEMANTIC_NAME_INVALID` - Adapter/stack name violates naming policy
- `VERSION_CONFLICT` - Resource version mismatch

#### Resource Errors (404)
- `NOT_FOUND` - Resource not found
- `ADAPTER_NOT_FOUND` - Adapter doesn't exist
- `DATASET_NOT_FOUND` - Dataset doesn't exist
- `WORKSPACE_NOT_FOUND` - Workspace doesn't exist
- `SESSION_NOT_FOUND` - Chat session doesn't exist
- `WORKER_NOT_FOUND` - Worker doesn't exist
- `MODEL_NOT_FOUND` - Model doesn't exist

#### Inference Errors (503, 429, 500)
- `MODEL_NOT_READY` (503) - Model not loaded (status != ready)
- `NO_COMPATIBLE_WORKER` (503) - No workers available
- `BACKPRESSURE` (429) - System overloaded, rate limit exceeded
- `REQUEST_TIMEOUT` (504) - Request took too long
- `ADAPTER_NOT_LOADABLE` (500) - Adapter can't be loaded
- `POLICY_HOOK_VIOLATION` (403) - Policy check failed
- `RAG_ERROR` (500) - Retrieval error
- `ROUTING_CHAIN_ERROR` (500) - Router error
- `DETERMINISM_VIOLATION` (500) - Non-deterministic behavior detected
- `ADAPTER_QUARANTINED` (503) - Adapter in quarantine due to failures

#### Training Errors (400, 500)
- `TRAINING_JOB_FAILED` - Training job failed
- `DATASET_INVALID` - Dataset validation failed
- `INSUFFICIENT_DATA` - Not enough training data
- `TRAINING_QUOTA_EXCEEDED` - Too many concurrent jobs
- `MODEL_EXPORT_FAILED` - Adapter packaging failed

#### Storage Errors (507, 500)
- `STORAGE_QUOTA_EXCEEDED` (507) - Tenant storage limit exceeded
- `UPLOAD_FAILED` (500) - File upload failed
- `CHUNKED_UPLOAD_INVALID` (400) - Chunked upload session invalid
- `FILE_TOO_LARGE` (413) - File exceeds size limit

#### Replay Errors (404, 409, 500)
- `REPLAY_NOT_AVAILABLE` (404) - Inference not replayable (metadata missing)
- `REPLAY_VERIFICATION_FAILED` (409) - Replay output doesn't match original
- `MANIFEST_NOT_FOUND` (404) - Manifest hash not found
- `SEED_DERIVATION_FAILED` (500) - Router seed cannot be derived

#### Monitoring Errors (400, 409)
- `ALERT_ALREADY_ACKNOWLEDGED` (409) - Alert already acknowledged
- `RULE_CONFLICT` (409) - Monitoring rule conflicts with existing rule
- `INVALID_METRIC` (400) - Metric name invalid or unsupported

#### Workspace Errors (403, 409)
- `WORKSPACE_MEMBER_EXISTS` (409) - Member already in workspace
- `WORKSPACE_RESOURCE_EXISTS` (409) - Resource already shared
- `LAST_ADMIN` (403) - Cannot remove last admin from workspace

#### System Errors (500, 503)
- `DATABASE_ERROR` (500) - Database operation failed
- `INTERNAL_ERROR` (500) - Unspecified internal error
- `SERVICE_UNAVAILABLE` (503) - Service unavailable
- `MAINTENANCE_MODE` (503) - System in maintenance mode
- `WORKER_CRASH` (500) - Worker process crashed
- `KV_ISOLATION_VIOLATION` (500) - KV isolation check failed

### Pagination

**Offset-based (most endpoints):**
```http
GET /v1/audit/logs?limit=50&offset=100
```

**Response includes:**
```json
{
  "data": [...],
  "pagination": {
    "limit": 50,
    "offset": 100,
    "total": 1234
  }
}
```

### Filtering

**Common query parameters:**
- `limit` - Max results (default: 100, max: 1000)
- `offset` - Skip N results (default: 0)
- `sort` - Sort field (e.g., `created_at`)
- `order` - Sort order (`asc` or `desc`)

**Resource-specific filters:**
```http
GET /v1/adapters?tier=1&framework=pytorch&status=loaded
GET /v1/training/jobs?status=running&created_after=2025-12-01
GET /v1/audit/logs?action=adapter.load&user_id=user-123
```

---

## Examples

### Complete Workflow: Train and Deploy Adapter

```bash
#!/bin/bash
set -e

BASE_URL="http://localhost:8080"
TOKEN="your-jwt-token"

# 1. Upload training dataset
echo "Uploading dataset..."
DATASET_ID=$(curl -X POST "$BASE_URL/v1/datasets/upload" \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@training_data.jsonl" \
  -F "name=customer-support-v1" \
  | jq -r '.id')

echo "Dataset ID: $DATASET_ID"

# 2. Start training job
echo "Starting training..."
JOB_ID=$(curl -X POST "$BASE_URL/v1/training/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "adapter_name": "customer-support-adapter",
    "dataset_id": "'$DATASET_ID'",
    "base_model": "qwen2.5-7b",
    "epochs": 3,
    "learning_rate": 0.0001,
    "lora_rank": 16
  }' \
  | jq -r '.job_id')

echo "Job ID: $JOB_ID"

# 3. Poll training status
echo "Waiting for training to complete..."
while true; do
  STATUS=$(curl -s "$BASE_URL/v1/training/jobs/$JOB_ID" \
    -H "Authorization: Bearer $TOKEN" \
    | jq -r '.status')

  if [ "$STATUS" = "completed" ]; then
    echo "Training completed!"
    break
  elif [ "$STATUS" = "failed" ]; then
    echo "Training failed!"
    exit 1
  fi

  echo "Status: $STATUS"
  sleep 10
done

# 4. Get adapter ID
ADAPTER_ID=$(curl -s "$BASE_URL/v1/training/jobs/$JOB_ID" \
  -H "Authorization: Bearer $TOKEN" \
  | jq -r '.adapter_id')

echo "Adapter ID: $ADAPTER_ID"

# 5. Load adapter
echo "Loading adapter..."
curl -X POST "$BASE_URL/v1/adapters/$ADAPTER_ID/load" \
  -H "Authorization: Bearer $TOKEN"

# 6. Run inference
echo "Running inference..."
curl -X POST "$BASE_URL/v1/infer" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "How do I reset my password?",
    "adapter_ids": ["'$ADAPTER_ID'"],
    "max_tokens": 100
  }' \
  | jq '.text'

echo "Deployment complete!"
```

### Streaming Inference with JavaScript

```javascript
async function streamInference(prompt) {
  const response = await fetch('http://localhost:8080/v1/infer/stream', {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${token}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      prompt,
      max_tokens: 200,
      temperature: 0.7
    })
  });

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split('\n');
    buffer = lines.pop(); // Keep incomplete line in buffer

    for (const line of lines) {
      if (line.startsWith('data: ')) {
        const data = JSON.parse(line.slice(6));

        if (data.text) {
          process.stdout.write(data.text);
        }

        if (data.done) {
          console.log('\nDone! Total tokens:', data.total_tokens);
        }
      }
    }
  }
}

streamInference('Write a haiku about machine learning');
```

### TypeScript Client SDK

```typescript
class AdapterOSClient {
  constructor(
    private baseURL: string,
    private token: string
  ) {}

  async login(email: string, password: string) {
    const response = await fetch(`${this.baseURL}/v1/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email, password })
    });

    const data = await response.json();
    this.token = data.token;
    return data;
  }

  async listAdapters(filters?: { tier?: number; framework?: string }) {
    const params = new URLSearchParams();
    if (filters?.tier) params.set('tier', String(filters.tier));
    if (filters?.framework) params.set('framework', filters.framework);

    const response = await fetch(
      `${this.baseURL}/v1/adapters?${params}`,
      {
        headers: { 'Authorization': `Bearer ${this.token}` }
      }
    );

    return response.json();
  }

  async infer(request: {
    prompt: string;
    adapterIds?: string[];
    maxTokens?: number;
  }) {
    const response = await fetch(`${this.baseURL}/v1/infer`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.token}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        prompt: request.prompt,
        adapter_ids: request.adapterIds || [],
        max_tokens: request.maxTokens || 100
      })
    });

    return response.json();
  }

  async startTraining(request: {
    adapterName: string;
    datasetId: string;
    baseModel: string;
    epochs: number;
  }) {
    const response = await fetch(`${this.baseURL}/v1/training/start`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.token}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        adapter_name: request.adapterName,
        dataset_id: request.datasetId,
        base_model: request.baseModel,
        epochs: request.epochs
      })
    });

    return response.json();
  }
}

// Usage
const client = new AdapterOSClient('http://localhost:8080', '');
await client.login('admin@example.com', 'password');

const adapters = await client.listAdapters({ tier: 1 });
console.log('Adapters:', adapters);

const result = await client.infer({
  prompt: 'Hello, world!',
  maxTokens: 50
});
console.log('Response:', result.text);
```

---

## Error Handling

### Retry Strategy

```typescript
async function withRetry<T>(
  fn: () => Promise<T>,
  maxAttempts = 3,
  backoffMs = 1000
): Promise<T> {
  let lastError: Error;

  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      return await fn();
    } catch (error) {
      lastError = error as Error;

      // Don't retry on client errors (4xx)
      if (error.status >= 400 && error.status < 500) {
        throw error;
      }

      if (attempt < maxAttempts) {
        const delay = backoffMs * Math.pow(2, attempt - 1);
        console.log(`Retry attempt ${attempt} after ${delay}ms`);
        await new Promise(resolve => setTimeout(resolve, delay));
      }
    }
  }

  throw lastError!;
}

// Usage
const result = await withRetry(
  () => client.infer({ prompt: 'Hello' }),
  3,
  1000
);
```

### Error Response Handling

```typescript
async function handleAPIError(response: Response) {
  if (!response.ok) {
    const error = await response.json();

    switch (error.code) {
      case 'SESSION_EXPIRED':
        // Refresh token and retry
        await refreshToken();
        return retryRequest();

      case 'MODEL_NOT_READY':
        // Wait for model to load
        await waitForModel();
        return retryRequest();

      case 'BACKPRESSURE':
        // Exponential backoff
        const retryAfter = response.headers.get('Retry-After');
        await sleep(parseInt(retryAfter) * 1000);
        return retryRequest();

      case 'VALIDATION_ERROR':
        // Don't retry, fix request
        throw new Error(`Validation failed: ${error.details}`);

      default:
        throw new Error(`API error: ${error.code} - ${error.error}`);
    }
  }

  return response.json();
}
```

### Health Check Integration

```typescript
async function waitForService(timeout = 60000) {
  const start = Date.now();

  while (Date.now() - start < timeout) {
    try {
      const response = await fetch('http://localhost:8080/readyz');

      if (response.ok) {
        console.log('Service is ready');
        return true;
      }
    } catch (error) {
      // Service not available yet
    }

    await new Promise(resolve => setTimeout(resolve, 1000));
  }

  throw new Error('Service did not become ready within timeout');
}

// Usage before running tests
await waitForService();
```

---

## Appendices

### Middleware Stack

Applied in order (outermost to innermost):

1. **client_ip_middleware** - Extract client IP
2. **security_headers_middleware** - CSP, X-Frame-Options, etc.
3. **request_size_limit_middleware** - Body size limits
4. **rate_limiting_middleware** - Per-tenant rate limits
5. **cors_layer** - CORS configuration
6. **TraceLayer** - HTTP request tracing
7. **auth_middleware** - JWT validation (protected routes only)

### Security Headers

All responses include:
- `Content-Security-Policy`
- `X-Frame-Options: DENY`
- `X-Content-Type-Options: nosniff`
- `Referrer-Policy: strict-origin-when-cross-origin`
- `Permissions-Policy` (restrictive)

### Rate Limiting

**Headers:**
- `X-RateLimit-Remaining` - Requests remaining
- `X-RateLimit-Reset` - Unix timestamp of reset
- `X-RateLimit-Limit` - Total limit
- `Retry-After` - Seconds until retry (when exceeded)

**Default Limits:**
- Per tenant: 1000 requests/minute
- Configurable via tenant settings

### API Statistics

| Category | Count |
|----------|-------|
| Total Endpoints | ~189 |
| Public Endpoints | 8 |
| Protected Endpoints | ~181 |
| SSE Streams | 7 |
| Handler Modules | 31 |

### Related Documentation

- [AGENTS.md](../AGENTS.md) - Development guide
- [ACCESS_CONTROL.md](ACCESS_CONTROL.md) - Access control (RBAC + tenant isolation)
- [POLICIES.md](POLICIES.md) - Policy enforcement
- [TELEMETRY_EVENTS.md](TELEMETRY_EVENTS.md) - Event tracking
- [API_GUIDES.md](API_GUIDES.md) - Workflow guides

---

**Document Version:** 1.1.0
**Last Updated:** 2025-12-20
**Maintained By:** MLNavigator Inc

**Changelog:**
- **v1.1.0 (2025-12-20):** Added comprehensive documentation for OpenAI compatibility, Replay & Determinism, RAG, extended Chat sessions, Policies, API Keys, Monitoring & Alerts, Plans & Orchestration, Workspaces, Admin Lifecycle, Storage & KV Isolation, Git Integration, Repository Management, Golden Runs & Promotion, Runtime & System State, Notifications, Tutorials, and Dashboard Configuration. Expanded error codes with HTTP status codes and categories.
- **v1.0.0 (2025-12-11):** Initial comprehensive API reference

For questions or support:
- Documentation: https://docs.adapteros.com
- Issues: GitHub Issues
- Security: security@adapteros.com
