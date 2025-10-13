# Security & Policy API

## Overview

Endpoints for managing code-specific security policies, path permissions, and safety checks. These ensure safe patch application and prevent unauthorized code modifications.

## Base Path

```
/v1/security/code
```

---

## Path Permission Endpoints

### GET /v1/security/repo-perms/{repo_id}

Get current path permissions for a repository.

**Request**:
```bash
GET /v1/security/repo-perms/acme%2Fpayments
```

**Response** (200 OK):
```json
{
  "repo_id": "acme/payments",
  "tenant_id": "tenant_acme",
  "allowlist": [
    "src/**",
    "lib/**",
    "tests/**"
  ],
  "denylist": [
    "**/.env*",
    "**/secrets/**",
    "**/*.pem",
    "**/*.key",
    ".github/workflows/**"
  ],
  "updated_at": "2025-10-01T09:00:00Z",
  "updated_by": "admin"
}
```

---

### PUT /v1/security/repo-perms/{repo_id}

Update path permissions for a repository.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "allowlist": [
    "src/**",
    "lib/**",
    "tests/**",
    "config/app.yml"
  ],
  "denylist": [
    "**/.env*",
    "**/secrets/**",
    "**/*.pem",
    "**/*.key",
    ".github/workflows/**",
    "config/database.yml"
  ]
}
```

**Response** (200 OK):
```json
{
  "status": "updated",
  "repo_id": "acme/payments",
  "updated_at": "2025-10-05T12:00:00Z"
}
```

**Errors**:
- `400`: Invalid glob patterns
- `403`: Not authorized to update permissions
- `409`: Allowlist and denylist overlap

---

### POST /v1/security/validate-path

Validate if a path is allowed for modification.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "repo_id": "acme/payments",
  "paths": [
    "src/payments/processor.py",
    "secrets/api_keys.json",
    ".env.production"
  ]
}
```

**Response** (200 OK):
```json
{
  "results": [
    {
      "path": "src/payments/processor.py",
      "allowed": true,
      "reason": "matches_allowlist"
    },
    {
      "path": "secrets/api_keys.json",
      "allowed": false,
      "reason": "matches_denylist",
      "matched_pattern": "**/secrets/**"
    },
    {
      "path": ".env.production",
      "allowed": false,
      "reason": "matches_denylist",
      "matched_pattern": "**/.env*"
    }
  ]
}
```

---

## Policy Management Endpoints

### GET /v1/policies/code/{tenant_id}

Get code-specific policies for a tenant.

**Request**:
```bash
GET /v1/policies/code/tenant_acme
```

**Response** (200 OK):
```json
{
  "tenant_id": "tenant_acme",
  "code_policy": {
    "evidence_min_spans": 1,
    "allow_auto_apply": false,
    "require_test_coverage": 0.8,
    "allow_external_deps": false,
    "secret_patterns": [
      "(?i)(api[_-]?key|password|secret|token)\\\\s*[:=]\\\\s*['\\\"][^'\\\"]{8,}['\\\"]",
      "(?i)(aws[_-]?access[_-]?key|aws[_-]?secret)",
      "(?i)(private[_-]?key)\\\\s*=",
      "-----BEGIN (RSA |EC |DSA )?PRIVATE KEY-----"
    ],
    "max_patch_size_lines": 500,
    "forbidden_operations": [
      "shell_escape",
      "eval",
      "exec_raw",
      "unsafe_deserialization"
    ],
    "require_review": {
      "database_migrations": true,
      "security_changes": true,
      "config_changes": true
    }
  },
  "updated_at": "2025-10-01T09:00:00Z"
}
```

---

### PUT /v1/policies/code/{tenant_id}

Update code policies for a tenant.

**Request**:
```json
{
  "code_policy": {
    "evidence_min_spans": 1,
    "allow_auto_apply": true,
    "require_test_coverage": 0.85,
    "allow_external_deps": false,
    "secret_patterns": [
      "(?i)(api[_-]?key|password|secret|token)\\\\s*[:=]\\\\s*['\\\"][^'\\\"]{8,}['\\\"]"
    ],
    "max_patch_size_lines": 500,
    "forbidden_operations": [
      "shell_escape",
      "eval",
      "exec_raw"
    ]
  }
}
```

**Response** (200 OK):
```json
{
  "status": "updated",
  "tenant_id": "tenant_acme",
  "updated_at": "2025-10-05T12:15:00Z"
}
```

---

## Secret Detection Endpoints

### POST /v1/security/scan-secrets

Scan code for potential secrets.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "content": "API_KEY = \"sk-1234567890abcdef\"\nPASSWORD = \"admin123\"",
  "file_path": "config/settings.py"
}
```

**Response** (200 OK):
```json
{
  "secrets_found": 2,
  "detections": [
    {
      "line": 1,
      "column": 0,
      "pattern": "api_key",
      "matched_text": "API_KEY = \"sk-***\"",
      "severity": "critical",
      "recommendation": "Use environment variable or secret manager"
    },
    {
      "line": 2,
      "column": 0,
      "pattern": "password",
      "matched_text": "PASSWORD = \"***\"",
      "severity": "critical",
      "recommendation": "Use environment variable or secret manager"
    }
  ],
  "safe": false
}
```

---

### POST /v1/security/scan-patch

Scan a patch for policy violations before applying.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "patch_set_id": "patch_abc123"
}
```

**Response** (200 OK):
```json
{
  "patch_set_id": "patch_abc123",
  "violations": [],
  "warnings": [
    {
      "type": "large_patch",
      "file": "src/payments/processor.py",
      "lines_changed": 87,
      "threshold": 50,
      "recommendation": "Consider breaking into smaller patches"
    }
  ],
  "checks": {
    "path_allowed": true,
    "no_secrets": true,
    "no_forbidden_ops": true,
    "size_ok": false,
    "no_migrations": true
  },
  "safe_to_apply": true,
  "requires_review": false
}
```

**With violations**:
```json
{
  "patch_set_id": "patch_abc456",
  "violations": [
    {
      "type": "secret_detected",
      "file": "config/settings.py",
      "line": 15,
      "pattern": "api_key",
      "matched_text": "API_KEY = \"sk-***\"",
      "severity": "critical"
    },
    {
      "type": "forbidden_operation",
      "file": "src/utils/dynamic.py",
      "line": 42,
      "operation": "eval",
      "code_snippet": "eval(user_input)",
      "severity": "critical"
    },
    {
      "type": "path_denied",
      "file": ".github/workflows/deploy.yml",
      "matched_pattern": ".github/workflows/**",
      "severity": "error"
    }
  ],
  "warnings": [],
  "checks": {
    "path_allowed": false,
    "no_secrets": false,
    "no_forbidden_ops": false,
    "size_ok": true,
    "no_migrations": true
  },
  "safe_to_apply": false,
  "requires_review": true
}
```

---

## Dependency Management Endpoints

### POST /v1/security/validate-dependency

Validate if a dependency can be added.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "repo_id": "acme/payments",
  "dependencies": [
    {
      "name": "requests",
      "version": "2.31.0",
      "language": "python"
    },
    {
      "name": "malicious-pkg",
      "version": "1.0.0",
      "language": "python"
    }
  ]
}
```

**Response** (200 OK):
```json
{
  "results": [
    {
      "name": "requests",
      "version": "2.31.0",
      "allowed": true,
      "reason": "pre_approved"
    },
    {
      "name": "malicious-pkg",
      "version": "1.0.0",
      "allowed": false,
      "reason": "policy_blocked_external_deps",
      "recommendation": "Contact admin to request approval"
    }
  ]
}
```

---

## Audit & Compliance Endpoints

### GET /v1/security/audit-log

Retrieve security audit log for code operations.

**Request**:
```bash
GET /v1/security/audit-log?tenant_id=tenant_acme&start=2025-10-01&end=2025-10-05&type=violation
```

**Response** (200 OK):
```json
{
  "entries": [
    {
      "event_id": "evt_abc123",
      "event_type": "security.violation",
      "tenant_id": "tenant_acme",
      "violation_type": "secret_detected",
      "severity": "critical",
      "details": {
        "file": "config/settings.py",
        "line": 15,
        "pattern": "api_key",
        "patch_set_id": "patch_abc456"
      },
      "action_taken": "patch_rejected",
      "timestamp": "2025-10-05T11:50:00Z"
    },
    {
      "event_id": "evt_def456",
      "event_type": "security.violation",
      "tenant_id": "tenant_acme",
      "violation_type": "path_denied",
      "severity": "error",
      "details": {
        "file": ".github/workflows/deploy.yml",
        "matched_pattern": ".github/workflows/**",
        "patch_set_id": "patch_abc456"
      },
      "action_taken": "patch_rejected",
      "timestamp": "2025-10-05T11:50:00Z"
    }
  ],
  "total": 2,
  "page": 1,
  "limit": 50
}
```

---

### POST /v1/security/incident-report

Create an incident report for a security issue.

**Request**:
```json
{
  "tenant_id": "tenant_acme",
  "incident_type": "unauthorized_patch_attempt",
  "severity": "high",
  "description": "Attempt to modify .github/workflows with malicious code",
  "evidence": {
    "patch_set_id": "patch_abc456",
    "violations": [
      {
        "type": "path_denied",
        "file": ".github/workflows/deploy.yml"
      },
      {
        "type": "forbidden_operation",
        "file": "src/utils/dynamic.py",
        "operation": "eval"
      }
    ],
    "user": "user@example.com",
    "timestamp": "2025-10-05T11:50:00Z"
  }
}
```

**Response** (201 Created):
```json
{
  "incident_id": "incident_abc123",
  "status": "created",
  "severity": "high",
  "created_at": "2025-10-05T11:55:00Z",
  "assigned_to": "security_team"
}
```

---

## Error Codes

- `POLICY_VIOLATION`: Code policy violated
- `SECRET_DETECTED`: Secret pattern found in code
- `PATH_DENIED`: File path not allowed for modification
- `FORBIDDEN_OPERATION`: Code contains forbidden operation (eval, exec, etc.)
- `DEPENDENCY_BLOCKED`: External dependency not allowed
- `SIZE_EXCEEDED`: Patch exceeds maximum size
- `REVIEW_REQUIRED`: Manual review required before apply

---

## Policy Enforcement Flow

1. **Before Patch Proposal**:
   - Validate context file paths
   - Check evidence requirements

2. **After Patch Generation**:
   - Scan for secrets
   - Check forbidden operations
   - Validate path permissions
   - Check patch size

3. **Before Apply**:
   - Re-run all checks (patches may change)
   - Verify test coverage if auto-apply enabled
   - Check for migrations or config changes

4. **On Violation**:
   - Reject operation
   - Log violation
   - Optionally create incident report
   - Notify tenant

---

## Audit Events

All security-related operations are logged:

**Secret detection**:
```json
{
  "event_type": "security.secret_detected",
  "tenant_id": "tenant_acme",
  "file": "config/settings.py",
  "line": 15,
  "pattern": "api_key",
  "severity": "critical",
  "timestamp": "2025-10-05T11:50:00Z"
}
```

**Policy violation**:
```json
{
  "event_type": "security.policy_violation",
  "tenant_id": "tenant_acme",
  "violation_type": "path_denied",
  "file": ".github/workflows/deploy.yml",
  "action_taken": "patch_rejected",
  "timestamp": "2025-10-05T11:50:00Z"
}
```

**Permission update**:
```json
{
  "event_type": "security.permissions_updated",
  "tenant_id": "tenant_acme",
  "repo_id": "acme/payments",
  "updated_by": "admin",
  "changes": {
    "allowlist_added": ["config/app.yml"],
    "denylist_added": ["config/database.yml"]
  },
  "timestamp": "2025-10-05T12:00:00Z"
}
```
