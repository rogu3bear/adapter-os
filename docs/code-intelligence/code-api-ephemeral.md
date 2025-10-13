# Ephemeral & Patch API

## Overview

Endpoints for commit delta pack (CDP) creation, ephemeral adapter management, and patch proposal/application. These enable per-commit context and code modification workflows.

## Base Path

```
/v1/code
```

---

## Commit Delta Pack (CDP) Endpoints

### POST /v1/code/commit-delta

Create a Commit Delta Pack for a specific commit.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "repo_id": "acme/payments",
  "commit": "abc123def456",
  "parent": "def456ghi789",
  "options": {
    "include_tests": true,
    "include_lint": true,
    "run_tests": true
  }
}
```

**Response** (202 Accepted):
```json
{
  "status": "accepted",
  "cdp_id": "cdp_abc123",
  "job_id": "cdp_job_12345",
  "estimated_duration_seconds": 30
}
```

**Query CDP status**:
```bash
GET /v1/code/commit-delta/{cdp_id}
```

**Response** (200 OK):
```json
{
  "cdp_id": "cdp_abc123",
  "repo_id": "acme/payments",
  "commit_sha": "abc123def456",
  "parent_sha": "def456ghi789",
  "status": "completed",
  "diff_summary": {
    "files_changed": 3,
    "insertions": 47,
    "deletions": 12,
    "changed_files": [
      "src/payments/processor.py",
      "tests/test_processor.py",
      "src/payments/models.py"
    ]
  },
  "changed_symbols": [
    {
      "symbol_id": "sym_abc123",
      "name": "process_payment",
      "change_type": "modified",
      "file": "src/payments/processor.py"
    },
    {
      "symbol_id": "sym_def456",
      "name": "test_process_payment_timeout",
      "change_type": "added",
      "file": "tests/test_processor.py"
    }
  ],
  "test_results": {
    "passed": 285,
    "failed": 2,
    "skipped": 0,
    "failures": [
      {
        "test": "test_process_payment_timeout",
        "file": "tests/test_processor.py",
        "error": "AssertionError: Expected PaymentTimeoutError",
        "traceback": "..."
      }
    ]
  },
  "lint_results": {
    "errors": 1,
    "warnings": 3,
    "issues": [
      {
        "file": "src/payments/processor.py",
        "line": 65,
        "column": 12,
        "severity": "error",
        "code": "E999",
        "message": "SyntaxError: invalid syntax"
      }
    ]
  },
  "hash_b3": "b3:fedcba9876543210...",
  "created_at": "2025-10-05T11:00:00Z",
  "expires_at": "2025-10-08T11:00:00Z"
}
```

---

## Ephemeral Adapter Endpoints

### POST /v1/code/ephemeral/create

Create an ephemeral adapter for a commit.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "repo_id": "acme/payments",
  "commit": "abc123def456",
  "mode": "micro_lora",
  "config": {
    "rank": 4,
    "alpha": 8,
    "target_modules": ["gate_proj", "up_proj", "down_proj"],
    "ttl_hours": 72
  },
  "cdp_id": "cdp_abc123"
}
```

**Modes**:
- `"zero_train"`: No training, only router priors from CDP
- `"micro_lora"`: Train rank 4-8 LoRA on synthetic pairs

**Response** (202 Accepted):
```json
{
  "status": "accepted",
  "adapter_id": "commit_abc123def456",
  "job_id": "ephemeral_job_12345",
  "mode": "micro_lora",
  "estimated_duration_seconds": 180
}
```

**Query ephemeral status**:
```bash
GET /v1/code/ephemeral/{adapter_id}
```

**Response** (200 OK):
```json
{
  "adapter_id": "commit_abc123def456",
  "repo_id": "acme/payments",
  "commit_sha": "abc123def456",
  "mode": "micro_lora",
  "status": "active",
  "hash_b3": "b3:abc123...",
  "rank": 4,
  "alpha": 8,
  "target_modules": ["gate_proj", "up_proj", "down_proj"],
  "ttl_seconds": 259200,
  "created_at": "2025-10-05T11:05:00Z",
  "expires_at": "2025-10-08T11:05:00Z",
  "remaining_seconds": 257400,
  "activation_count": 47,
  "metadata": {
    "pr_id": "123",
    "branch": "fix/payment-timeout",
    "training_pairs": 42,
    "training_loss": 0.023
  }
}
```

---

### POST /v1/code/ephemeral/attach

Attach ephemeral adapter to active worker.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "adapter_id": "commit_abc123def456",
  "cp_pointer": "code"
}
```

**Response** (200 OK):
```json
{
  "status": "attached",
  "adapter_id": "commit_abc123def456",
  "cp_pointer": "code",
  "message": "Ephemeral adapter hot-attached to worker"
}
```

---

### POST /v1/code/ephemeral/evict

Manually evict an ephemeral adapter.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "adapter_id": "commit_abc123def456",
  "reason": "PR merged"
}
```

**Response** (200 OK):
```json
{
  "status": "evicted",
  "adapter_id": "commit_abc123def456",
  "evicted_at": "2025-10-05T11:30:00Z",
  "reason": "PR merged"
}
```

---

### GET /v1/code/ephemeral/list

List active ephemeral adapters for a repository.

**Request**:
```bash
GET /v1/code/ephemeral/list?tenant_id=tenant_acme&repo_id=acme%2Fpayments
```

**Response** (200 OK):
```json
{
  "ephemeral_adapters": [
    {
      "adapter_id": "commit_abc123def456",
      "commit_sha": "abc123def456",
      "mode": "micro_lora",
      "status": "active",
      "created_at": "2025-10-05T11:05:00Z",
      "expires_at": "2025-10-08T11:05:00Z",
      "remaining_seconds": 257400,
      "metadata": {
        "pr_id": "123",
        "branch": "fix/payment-timeout"
      }
    },
    {
      "adapter_id": "commit_def456ghi789",
      "commit_sha": "def456ghi789",
      "mode": "zero_train",
      "status": "active",
      "created_at": "2025-10-05T09:30:00Z",
      "expires_at": "2025-10-07T09:30:00Z",
      "remaining_seconds": 168000,
      "metadata": {
        "pr_id": "122",
        "branch": "feature/new-payment-method"
      }
    }
  ],
  "total": 2
}
```

---

## Patch Endpoints

### POST /v1/code/patch/propose

Propose a patch based on a request and context.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "repo_id": "acme/payments",
  "commit": "abc123def456",
  "request": {
    "prompt": "Fix the failing test test_process_payment_timeout by handling the timeout case",
    "context_files": [
      "src/payments/processor.py",
      "tests/test_processor.py"
    ],
    "targets": [
      {
        "file": "src/payments/processor.py",
        "symbol": "process_payment"
      }
    ],
    "goals": ["fix_test", "add_timeout_handling"]
  },
  "options": {
    "max_hunks": 5,
    "max_lines_per_hunk": 50,
    "dry_run": false
  }
}
```

**Response** (200 OK):
```json
{
  "patch_set_id": "patch_abc123",
  "status": "proposed",
  "patches": [
    {
      "file": "src/payments/processor.py",
      "hunks": [
        {
          "start_line": 60,
          "end_line": 70,
          "original": "def process_payment(amount, currency):\n    result = gateway.charge(...)\n    return result",
          "modified": "def process_payment(amount, currency):\n    try:\n        result = gateway.charge(..., timeout=30)\n        return result\n    except TimeoutError:\n        raise PaymentTimeoutError('Payment processing timed out')",
          "reason": "Add timeout handling to satisfy test requirement"
        }
      ]
    }
  ],
  "rationale": "The test expects a PaymentTimeoutError when payment processing takes too long. Added try/except block with timeout parameter to gateway call.",
  "citations": [
    {
      "type": "code_span",
      "file": "src/payments/processor.py",
      "span": {"start_line": 58, "end_line": 112},
      "symbol": "process_payment",
      "relevance": "target_function"
    },
    {
      "type": "test_log",
      "file": "tests/test_processor.py",
      "test": "test_process_payment_timeout",
      "error": "AssertionError: Expected PaymentTimeoutError",
      "relevance": "failing_test"
    },
    {
      "type": "framework_doc",
      "framework": "django",
      "topic": "timeout_handling",
      "snippet": "gateway.charge() accepts timeout parameter",
      "relevance": "api_reference"
    }
  ],
  "trace": {
    "top_adapters": [
      ["commit_abc123def456", 0.52],
      ["codebase_acme_payments_v7", 0.31],
      ["code_lang_v1", 0.17]
    ],
    "router_features": {
      "symbol_hits": 3.0,
      "commit_hint": 1.0,
      "framework_prior": ["pytest"],
      "prompt_verb": "fix"
    },
    "event_hash": "b3:xyz789..."
  }
}
```

---

### POST /v1/code/patch/apply

Apply a proposed patch (with dry-run option).

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "patch_set_id": "patch_abc123",
  "dry_run": true,
  "options": {
    "run_tests": true,
    "run_linter": true
  }
}
```

**Response** (200 OK):
```json
{
  "status": "dry_run_success",
  "patch_set_id": "patch_abc123",
  "dry_run_results": {
    "apply_success": true,
    "test_results": {
      "passed": 287,
      "failed": 0,
      "skipped": 0,
      "newly_passing": ["test_process_payment_timeout"]
    },
    "lint_results": {
      "errors": 0,
      "warnings": 3,
      "delta": {
        "errors": -1,
        "warnings": 0
      }
    },
    "policy_checks": {
      "path_allowed": true,
      "secret_detected": false,
      "size_ok": true
    }
  },
  "recommendation": "safe_to_apply"
}
```

**With dry_run=false**:
```json
{
  "status": "applied",
  "patch_set_id": "patch_abc123",
  "commit_sha": "new_commit_abc456",
  "files_modified": ["src/payments/processor.py"],
  "test_results": {
    "passed": 287,
    "failed": 0
  },
  "applied_at": "2025-10-05T11:45:00Z"
}
```

---

### GET /v1/code/patch/{patch_set_id}

Retrieve details of a patch set.

**Request**:
```bash
GET /v1/code/patch/patch_abc123
```

**Response** (200 OK):
```json
{
  "patch_set_id": "patch_abc123",
  "repo_id": "acme/payments",
  "commit": "abc123def456",
  "status": "proposed",
  "patches": [...],
  "rationale": "...",
  "citations": [...],
  "created_at": "2025-10-05T11:40:00Z"
}
```

---

## Refusal Responses

When evidence is insufficient, the system returns a structured refusal:

```json
{
  "status": "insufficient_evidence",
  "needed": ["file_path", "symbol", "test_target"],
  "hint": "Please specify the target file and function name",
  "available_context": {
    "symbol_hits": 0,
    "retrieval_count": 0,
    "framework_detected": ["django", "pytest"]
  },
  "suggestions": [
    "Provide file path: src/payments/processor.py",
    "Specify symbol: process_payment",
    "Include test name: test_process_payment_timeout"
  ]
}
```

---

## Error Codes

- `CDP_CREATION_FAILED`: Failed to create commit delta pack
- `EPHEMERAL_TRAINING_FAILED`: LoRA training failed
- `PATCH_APPLY_FAILED`: Patch could not be applied
- `TESTS_FAILED`: Tests failed after patch application
- `POLICY_VIOLATION`: Patch violates code policy (secret, path, size)
- `TTL_EXPIRED`: Ephemeral adapter has expired
- `INVALID_COMMIT`: Commit SHA not found

---

## Audit Logging

All ephemeral and patch operations are logged:

**CDP creation**:
```json
{
  "event_type": "code.cdp.created",
  "tenant_id": "tenant_acme",
  "repo_id": "acme/payments",
  "commit_sha": "abc123def456",
  "files_changed": 3,
  "test_failures": 2,
  "cdp_id": "cdp_abc123",
  "timestamp": "2025-10-05T11:00:00Z"
}
```

**Ephemeral creation**:
```json
{
  "event_type": "code.ephemeral.created",
  "tenant_id": "tenant_acme",
  "adapter_id": "commit_abc123def456",
  "mode": "micro_lora",
  "rank": 4,
  "ttl_seconds": 259200,
  "timestamp": "2025-10-05T11:05:00Z"
}
```

**Patch proposed**:
```json
{
  "event_type": "code.patch.proposed",
  "tenant_id": "tenant_acme",
  "patch_set_id": "patch_abc123",
  "files_affected": ["src/payments/processor.py"],
  "hunks": 1,
  "trace_event_hash": "b3:xyz789...",
  "timestamp": "2025-10-05T11:40:00Z"
}
```

**Patch applied**:
```json
{
  "event_type": "code.patch.applied",
  "tenant_id": "tenant_acme",
  "patch_set_id": "patch_abc123",
  "new_commit_sha": "new_commit_abc456",
  "dry_run": false,
  "tests_passed": true,
  "timestamp": "2025-10-05T11:45:00Z"
}
```
