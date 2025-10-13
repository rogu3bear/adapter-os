# Code Registry API

## Overview

REST API endpoints for repository registration, scanning, and CodeGraph management. All endpoints follow AdapterOS conventions: UDS-only, tenant-scoped, with audit logging.

## Base Path

```
/v1/code
```

## Authentication

All requests require tenant authentication via standard AdapterOS headers:

```
X-Tenant-ID: tenant_acme
X-Auth-Token: <token>
```

---

## Endpoints

### POST /v1/code/register-repo

Register a new repository for code intelligence.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "repo_id": "acme/payments",
  "path": "/repos/acme/payments",
  "languages": ["Python", "TypeScript"],
  "default_branch": "main"
}
```

**Response** (202 Accepted):
```json
{
  "status": "accepted",
  "repo_id": "acme/payments",
  "message": "Repository registered successfully"
}
```

**Errors**:
- `400`: Invalid repo path or languages
- `403`: Tenant does not have access to path
- `409`: Repository already registered

---

### POST /v1/code/scan

Trigger a full repository scan and index build.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "repo_id": "acme/payments",
  "commit": "abc123def456",
  "full_scan": true
}
```

**Response** (202 Accepted):
```json
{
  "status": "accepted",
  "job_id": "scan_job_12345",
  "repo_id": "acme/payments",
  "commit": "abc123def456",
  "estimated_duration_seconds": 120
}
```

**Query job status**:
```bash
GET /v1/code/scan/{job_id}
```

**Response** (200 OK):
```json
{
  "job_id": "scan_job_12345",
  "status": "completed",
  "progress": {
    "current_stage": "package_and_store",
    "percentage": 100
  },
  "result": {
    "code_graph_hash": "b3:abc123...",
    "symbol_index_hash": "b3:def456...",
    "vector_index_hash": "b3:ghi789...",
    "test_map_hash": "b3:jkl012...",
    "file_count": 142,
    "symbol_count": 1834,
    "test_count": 287
  },
  "started_at": "2025-10-05T10:30:00Z",
  "completed_at": "2025-10-05T10:32:15Z"
}
```

**Errors**:
- `400`: Invalid commit SHA
- `404`: Repository not registered
- `409`: Scan already in progress

---

### GET /v1/code/graph/{repo_id}@{commit}

Retrieve CodeGraph metadata for a specific commit.

**Request**:
```bash
GET /v1/code/graph/acme%2Fpayments@abc123def456
```

**Response** (200 OK):
```json
{
  "code_graph_id": "graph_acme_payments_abc123",
  "repo_id": "acme/payments",
  "commit_sha": "abc123def456",
  "hash_b3": "b3:fedcba9876543210...",
  "file_count": 142,
  "symbol_count": 1834,
  "test_count": 287,
  "languages": ["Python", "TypeScript"],
  "frameworks": [
    {"name": "django", "version": "4.2"},
    {"name": "pytest", "version": "7.4"}
  ],
  "size_bytes": 5242880,
  "created_at": "2025-10-05T10:32:15Z",
  "indices": {
    "symbol_index_hash": "b3:def456...",
    "vector_index_hash": "b3:ghi789...",
    "test_map_hash": "b3:jkl012..."
  }
}
```

**Errors**:
- `404`: CodeGraph not found for commit

---

### GET /v1/code/repos/{tenant_id}

List all repositories for a tenant.

**Request**:
```bash
GET /v1/code/repos/tenant_acme?page=1&limit=50
```

**Response** (200 OK):
```json
{
  "repos": [
    {
      "repo_id": "acme/payments",
      "path": "/repos/acme/payments",
      "languages": ["Python", "TypeScript"],
      "frameworks": [
        {"name": "django", "version": "4.2"},
        {"name": "pytest", "version": "7.4"}
      ],
      "default_branch": "main",
      "latest_scan_commit": "abc123def456",
      "latest_scan_at": "2025-10-05T10:32:15Z",
      "created_at": "2025-10-01T09:00:00Z"
    },
    {
      "repo_id": "acme/frontend",
      "path": "/repos/acme/frontend",
      "languages": ["TypeScript"],
      "frameworks": [
        {"name": "react", "version": "18"},
        {"name": "nextjs", "version": "14"}
      ],
      "default_branch": "main",
      "latest_scan_commit": "def456ghi789",
      "latest_scan_at": "2025-10-04T15:20:00Z",
      "created_at": "2025-10-01T09:15:00Z"
    }
  ],
  "pagination": {
    "page": 1,
    "limit": 50,
    "total": 2
  }
}
```

---

### DELETE /v1/code/repo/{repo_id}

Unregister a repository and delete all associated artifacts.

**Request**:
```bash
DELETE /v1/code/repo/acme%2Fpayments?confirm=true
```

**Response** (200 OK):
```json
{
  "status": "deleted",
  "repo_id": "acme/payments",
  "artifacts_deleted": {
    "code_graphs": 5,
    "symbol_indices": 5,
    "vector_indices": 5,
    "test_maps": 5,
    "adapters": 2
  }
}
```

**Errors**:
- `400`: Missing confirmation flag
- `403`: Not authorized to delete
- `404`: Repository not found

---

### POST /v1/code/symbol-search

Search for symbols across registered repositories.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "repo_id": "acme/payments",
  "query": "process_payment OR Payment",
  "filters": {
    "kind": ["Function", "Class"],
    "file_pattern": "src/**"
  },
  "limit": 20
}
```

**Response** (200 OK):
```json
{
  "symbols": [
    {
      "symbol_id": "sym_abc123",
      "name": "process_payment",
      "kind": "Function",
      "signature": "def process_payment(amount: Decimal, currency: str) -> PaymentResult",
      "docstring": "Process a payment transaction.",
      "file_path": "src/payments/processor.py",
      "span": {
        "start_line": 58,
        "start_col": 0,
        "end_line": 112,
        "end_col": 0
      },
      "score": 0.95
    },
    {
      "symbol_id": "sym_def456",
      "name": "Payment",
      "kind": "Class",
      "signature": "class Payment(models.Model)",
      "docstring": "Represents a payment in the system.",
      "file_path": "src/payments/models.py",
      "span": {
        "start_line": 12,
        "start_col": 0,
        "end_line": 45,
        "end_col": 0
      },
      "score": 0.89
    }
  ],
  "total": 2,
  "query_time_ms": 23
}
```

---

### POST /v1/code/semantic-search

Semantic (vector) search for code chunks.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "repo_id": "acme/payments",
  "query": "How do we handle payment timeouts?",
  "filters": {
    "language": "Python",
    "is_test": false
  },
  "k": 5
}
```

**Response** (200 OK):
```json
{
  "chunks": [
    {
      "chunk_id": "chunk_abc123",
      "file_path": "src/payments/processor.py",
      "symbol_name": "process_payment",
      "text": "def process_payment(...):\n    ...\n    # Handle timeout\n    if elapsed > TIMEOUT_SECONDS:\n        raise PaymentTimeoutError(...)\n    ...",
      "span": {
        "start_line": 58,
        "start_col": 0,
        "end_line": 68,
        "end_col": 0
      },
      "score": 0.87,
      "metadata": {
        "language": "Python",
        "symbol_kind": "Function",
        "is_test": false
      }
    }
  ],
  "total": 5,
  "query_time_ms": 45
}
```

---

### GET /v1/code/test-impact

Compute test impact for changed files.

**Request**:
```bash
GET /v1/code/test-impact/acme%2Fpayments@abc123def456?files=src/payments/processor.py,src/payments/models.py
```

**Response** (200 OK):
```json
{
  "impacted_tests": [
    {
      "test_id": "test_abc123",
      "name": "test_process_payment_success",
      "file_path": "tests/test_processor.py",
      "reason": "directly_covers",
      "covered_symbols": ["process_payment"]
    },
    {
      "test_id": "test_def456",
      "name": "test_payment_model_validation",
      "file_path": "tests/test_models.py",
      "reason": "directly_covers",
      "covered_symbols": ["Payment"]
    },
    {
      "test_id": "test_ghi789",
      "name": "test_api_create_payment",
      "file_path": "tests/test_api.py",
      "reason": "calls_changed_symbol",
      "covered_symbols": ["process_payment"]
    }
  ],
  "total": 3,
  "coverage": {
    "file_coverage": {
      "src/payments/processor.py": 2,
      "src/payments/models.py": 1
    }
  }
}
```

---

### GET /v1/code/frameworks/{repo_id}

Get detected frameworks for a repository.

**Request**:
```bash
GET /v1/code/frameworks/acme%2Fpayments
```

**Response** (200 OK):
```json
{
  "repo_id": "acme/payments",
  "frameworks": [
    {
      "name": "django",
      "version": "4.2",
      "config_files": [
        "backend/settings.py",
        "backend/urls.py",
        "manage.py"
      ]
    },
    {
      "name": "pytest",
      "version": "7.4",
      "config_files": [
        "pytest.ini",
        "pyproject.toml"
      ]
    }
  ]
}
```

---

## Error Responses

Standard error format:

```json
{
  "error": {
    "code": "REPO_NOT_FOUND",
    "message": "Repository 'acme/payments' not found",
    "details": {
      "repo_id": "acme/payments",
      "tenant_id": "tenant_acme"
    }
  }
}
```

### Error Codes

- `INVALID_REQUEST`: Malformed request body
- `UNAUTHORIZED`: Missing or invalid auth token
- `FORBIDDEN`: Tenant does not have permission
- `REPO_NOT_FOUND`: Repository not registered
- `GRAPH_NOT_FOUND`: CodeGraph not found for commit
- `SCAN_IN_PROGRESS`: Another scan is already running
- `SCAN_FAILED`: Scan job failed (check job status for details)
- `STORAGE_ERROR`: CAS or registry error

---

## Rate Limiting

- Symbol search: 100 requests/minute per tenant
- Semantic search: 50 requests/minute per tenant
- Scan requests: 10 requests/minute per tenant

Headers:
```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 87
X-RateLimit-Reset: 1696510800
```

---

## Audit Logging

All API calls are logged in telemetry with:
- Tenant ID
- Endpoint path
- Request parameters (sanitized)
- Response status
- Timing
- CPID (if applicable)

Example event:
```json
{
  "event_type": "code.api.request",
  "tenant_id": "tenant_acme",
  "endpoint": "/v1/code/scan",
  "method": "POST",
  "repo_id": "acme/payments",
  "status": 202,
  "duration_ms": 234,
  "timestamp": "2025-10-05T10:30:00Z"
}
```
