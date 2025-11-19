# Golden Run Promotion Workflow API

**Status:** Implementation Complete
**Version:** v1.0
**Last Updated:** 2025-11-19
**Author:** Claude (Agent 5)

## Overview

The Promotion Workflow API provides endpoints for managing the promotion of golden runs through validation gates, approval workflow, and deployment to staging/production environments.

## Architecture

### Workflow Stages

```
1. Request Promotion → 2. Gate Validation → 3. Approval → 4. Deployment
         ↓                     ↓                  ↓            ↓
    Create Request       Run Policy Checks    Sign Approval  Update Stage
                         Run Hash Validation
                         Run Determinism Check
```

### Database Schema

The workflow uses the following tables (migration `0076_golden_run_promotions.sql`):

- **golden_run_promotion_requests**: Tracks all promotion requests
- **golden_run_promotion_approvals**: Records approval/rejection actions
- **golden_run_promotion_gates**: Stores gate validation results
- **golden_run_promotion_history**: Audit trail of promotions and rollbacks
- **golden_run_stages**: Tracks active golden run per stage (staging/production)

## API Endpoints

### 1. Request Promotion

**Endpoint:** `POST /v1/golden/:runId/promote`

**Description:** Initiates a promotion request for a golden run. This triggers automatic gate validation in the background.

**RBAC:** Requires `PromotionManage` permission (Admin, Operator roles)

**Path Parameters:**
- `runId` (string, required): The ID of the golden run to promote

**Request Body:**
```json
{
  "target_stage": "staging",  // "staging" or "production"
  "notes": "Promoting after QA verification"  // optional
}
```

**Response:** `200 OK`
```json
{
  "request_id": "promo-my-run-uuid-v4",
  "golden_run_id": "my-run",
  "target_stage": "staging",
  "status": "pending",
  "created_at": "2025-11-19T10:30:00Z"
}
```

**Error Responses:**
- `400 Bad Request`: Invalid target_stage or golden run doesn't exist
- `403 Forbidden`: Insufficient permissions
- `404 Not Found`: Golden run not found

**Example:**
```bash
curl -X POST https://api.adapteros.dev/v1/golden/my-run-001/promote \
  -H "Authorization: Bearer $JWT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "target_stage": "staging",
    "notes": "Promoting after successful testing"
  }'
```

---

### 2. Get Promotion Status

**Endpoint:** `GET /v1/golden/:runId/promotion`

**Description:** Retrieves the current promotion status, including gate results and approval records.

**RBAC:** Requires `PromotionManage` permission

**Path Parameters:**
- `runId` (string, required): The ID of the golden run

**Response:** `200 OK`
```json
{
  "request_id": "promo-my-run-uuid-v4",
  "golden_run_id": "my-run",
  "target_stage": "staging",
  "status": "pending",  // "pending", "approved", "rejected", "promoted", "rolled_back"
  "requester_email": "engineer@example.com",
  "created_at": "2025-11-19T10:30:00Z",
  "updated_at": "2025-11-19T10:35:00Z",
  "notes": "Promoting after QA verification",
  "gates": [
    {
      "gate_name": "hash_validation",
      "status": "passed",
      "passed": true,
      "details": {
        "bundle_hash": "b3:abc123...",
        "layer_count": 32
      },
      "error_message": null,
      "checked_at": "2025-11-19T10:30:05Z"
    },
    {
      "gate_name": "policy_check",
      "status": "passed",
      "passed": true,
      "details": {
        "policies_checked": 23,
        "policies_passed": 23
      },
      "error_message": null,
      "checked_at": "2025-11-19T10:30:10Z"
    },
    {
      "gate_name": "determinism_check",
      "status": "passed",
      "passed": true,
      "details": {
        "max_epsilon": 1.2e-7,
        "mean_epsilon": 3.4e-8
      },
      "error_message": null,
      "checked_at": "2025-11-19T10:30:15Z"
    }
  ],
  "approvals": []
}
```

**Error Responses:**
- `404 Not Found`: No promotion request found for this golden run

**Example:**
```bash
curl https://api.adapteros.dev/v1/golden/my-run-001/promotion \
  -H "Authorization: Bearer $JWT_TOKEN"
```

---

### 3. Approve or Reject Promotion

**Endpoint:** `POST /v1/golden/:runId/approve`

**Description:** Records an approval or rejection decision. If approved, automatically executes the promotion.

**RBAC:** Requires `PromotionManage` permission (Admin only for production)

**Path Parameters:**
- `runId` (string, required): The ID of the golden run

**Request Body:**
```json
{
  "action": "approve",  // "approve" or "reject"
  "message": "All gates passed, approved for staging deployment"
}
```

**Response:** `200 OK`
```json
{
  "request_id": "promo-my-run-uuid-v4",
  "status": "approved",  // "approved" or "rejected"
  "signature": "sig-abc123..."  // Ed25519 signature
}
```

**Error Responses:**
- `400 Bad Request`: Invalid action or promotion already processed
- `403 Forbidden`: Insufficient permissions
- `404 Not Found`: Promotion request not found

**Example:**
```bash
# Approve
curl -X POST https://api.adapteros.dev/v1/golden/my-run-001/approve \
  -H "Authorization: Bearer $JWT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "action": "approve",
    "message": "All quality gates passed"
  }'

# Reject
curl -X POST https://api.adapteros.dev/v1/golden/my-run-001/approve \
  -H "Authorization: Bearer $JWT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "action": "reject",
    "message": "Determinism check failed"
  }'
```

---

### 4. Get Gate Status

**Endpoint:** `GET /v1/golden/:runId/gates`

**Description:** Retrieves only the gate validation results for a promotion request.

**RBAC:** Requires `PromotionManage` permission

**Path Parameters:**
- `runId` (string, required): The ID of the golden run

**Response:** `200 OK`
```json
[
  {
    "gate_name": "hash_validation",
    "status": "passed",
    "passed": true,
    "details": {
      "bundle_hash": "b3:abc123...",
      "layer_count": 32
    },
    "error_message": null,
    "checked_at": "2025-11-19T10:30:05Z"
  },
  {
    "gate_name": "policy_check",
    "status": "failed",
    "passed": false,
    "details": null,
    "error_message": "Policy egress_control_v2 failed validation",
    "checked_at": "2025-11-19T10:30:10Z"
  }
]
```

**Error Responses:**
- `404 Not Found`: No promotion request or gates found

**Example:**
```bash
curl https://api.adapteros.dev/v1/golden/my-run-001/gates \
  -H "Authorization: Bearer $JWT_TOKEN"
```

---

### 5. Rollback Promotion

**Endpoint:** `POST /v1/golden/:stage/rollback`

**Description:** Rolls back a stage to the previous golden run version.

**RBAC:** Requires `PromotionManage` permission (Admin only for production)

**Path Parameters:**
- `stage` (string, required): The stage to rollback ("staging" or "production")

**Request Body:**
```json
{
  "reason": "Critical bug found in production - reverting to previous version"
}
```

**Response:** `200 OK`
```json
{
  "stage": "production",
  "rolled_back_to": "previous-run-001",
  "rolled_back_from": "current-run-002",
  "reason": "Critical bug found in production - reverting to previous version"
}
```

**Error Responses:**
- `400 Bad Request`: Invalid stage or no previous version available
- `403 Forbidden`: Insufficient permissions
- `404 Not Found`: Stage not found

**Example:**
```bash
curl -X POST https://api.adapteros.dev/v1/golden/production/rollback \
  -H "Authorization: Bearer $JWT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "reason": "Critical performance regression detected"
  }'
```

---

## Validation Gates

### Gate Types

The promotion workflow includes three automatic validation gates:

#### 1. Hash Validation Gate
- **Purpose:** Verify bundle integrity and adapter hashes
- **Checks:**
  - Bundle hash exists and is valid
  - All adapter hashes are present
  - Layer count matches expectations
- **Pass Criteria:** All hashes valid, no empty values

#### 2. Policy Check Gate
- **Purpose:** Ensure compliance with all 23 canonical policies
- **Checks:**
  - Egress control policy
  - Determinism policy
  - Evidence policy
  - All other policy pack rules
- **Pass Criteria:** All 23 policies pass validation

#### 3. Determinism Check Gate
- **Purpose:** Verify deterministic execution guarantees
- **Checks:**
  - Max epsilon < 1e-6
  - Mean epsilon acceptable
  - Epsilon statistics available
- **Pass Criteria:** Epsilon values within acceptable bounds

### Gate Execution

Gates are executed **asynchronously** when a promotion request is created. The workflow:

1. Create promotion request → Returns immediately with `status: "pending"`
2. Background task spawns → Runs all gates in parallel
3. Results recorded in database → `golden_run_promotion_gates` table
4. Frontend polls `/v1/golden/:runId/gates` → Shows real-time progress

### Gate Overrides

**Policy:** Gates cannot be overridden without Admin approval. If a gate fails:
- Promotion status remains `pending`
- Approver must review failure details
- Can reject with explanation
- Cannot approve unless gates pass (enforced in handler)

---

## RBAC Integration

### Permission Requirements

| Endpoint | Permission | Roles |
|----------|-----------|-------|
| POST `/v1/golden/:runId/promote` | `PromotionManage` | Admin, Operator |
| GET `/v1/golden/:runId/promotion` | `PromotionManage` | Admin, Operator, SRE, Compliance |
| POST `/v1/golden/:runId/approve` | `PromotionManage` | Admin, Operator |
| GET `/v1/golden/:runId/gates` | `PromotionManage` | Admin, Operator, SRE, Compliance |
| POST `/v1/golden/:stage/rollback` | `PromotionManage` | Admin, Operator |

**Note:** For production stage operations, recommend Admin-only via additional policy layer.

### Audit Logging

All promotion actions are logged using the audit helper:

```rust
use crate::audit_helper::{actions, log_success, resources};

log_success(
    &state.db,
    &claims,
    actions::PROMOTION_EXECUTE,
    resources::PROMOTION,
    Some(&request_id),
).await;
```

**Audit Actions:**
- `promotion.execute` - Promotion requested
- `promotion.rollback` - Stage rolled back
- `promotion.approve` - Promotion approved
- `promotion.reject` - Promotion rejected

Query audit logs:
```bash
curl "https://api.adapteros.dev/v1/audit/logs?action=promotion.execute&limit=50" \
  -H "Authorization: Bearer $JWT_TOKEN"
```

---

## Frontend Integration

### React Hook Example

```typescript
import { useQuery, useMutation } from '@tanstack/react-query';

// Request promotion
const useRequestPromotion = () => {
  return useMutation({
    mutationFn: async ({ runId, targetStage, notes }: {
      runId: string;
      targetStage: 'staging' | 'production';
      notes?: string;
    }) => {
      const response = await fetch(`/v1/golden/${runId}/promote`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${getToken()}`,
        },
        body: JSON.stringify({ target_stage: targetStage, notes }),
      });
      return response.json();
    },
  });
};

// Poll promotion status
const usePromotionStatus = (runId: string, enabled: boolean) => {
  return useQuery({
    queryKey: ['promotion-status', runId],
    queryFn: async () => {
      const response = await fetch(`/v1/golden/${runId}/promotion`, {
        headers: { 'Authorization': `Bearer ${getToken()}` },
      });
      return response.json();
    },
    refetchInterval: enabled ? 2000 : false, // Poll every 2s when enabled
    enabled,
  });
};

// Approve promotion
const useApprovePromotion = () => {
  return useMutation({
    mutationFn: async ({ runId, action, message }: {
      runId: string;
      action: 'approve' | 'reject';
      message: string;
    }) => {
      const response = await fetch(`/v1/golden/${runId}/approve`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${getToken()}`,
        },
        body: JSON.stringify({ action, message }),
      });
      return response.json();
    },
  });
};

// Rollback stage
const useRollback = () => {
  return useMutation({
    mutationFn: async ({ stage, reason }: {
      stage: 'staging' | 'production';
      reason: string;
    }) => {
      const response = await fetch(`/v1/golden/${stage}/rollback`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${getToken()}`,
        },
        body: JSON.stringify({ reason }),
      });
      return response.json();
    },
  });
};
```

### Component Example

```typescript
import React from 'react';
import { usePromotionStatus, useApprovePromotion } from './hooks';

export const PromotionPanel: React.FC<{ runId: string }> = ({ runId }) => {
  const { data, isLoading } = usePromotionStatus(runId, true);
  const approvePromotion = useApprovePromotion();

  if (isLoading) return <div>Loading...</div>;
  if (!data) return <div>No promotion found</div>;

  const allGatesPassed = data.gates.every(g => g.passed);

  return (
    <div className="promotion-panel">
      <h2>Promotion Status: {data.status}</h2>

      <div className="gates">
        <h3>Validation Gates</h3>
        {data.gates.map(gate => (
          <div key={gate.gate_name} className={gate.passed ? 'passed' : 'failed'}>
            <span>{gate.gate_name}</span>
            <span>{gate.status}</span>
            {gate.error_message && <p className="error">{gate.error_message}</p>}
          </div>
        ))}
      </div>

      {allGatesPassed && data.status === 'pending' && (
        <div className="actions">
          <button onClick={() => approvePromotion.mutate({
            runId,
            action: 'approve',
            message: 'All gates passed, approved for deployment'
          })}>
            Approve Promotion
          </button>
          <button onClick={() => approvePromotion.mutate({
            runId,
            action: 'reject',
            message: 'Manual review required'
          })}>
            Reject
          </button>
        </div>
      )}

      {data.approvals.length > 0 && (
        <div className="approvals">
          <h3>Approval History</h3>
          {data.approvals.map((approval, idx) => (
            <div key={idx}>
              <p>{approval.action} by {approval.approver_email}</p>
              <p>{approval.message}</p>
              <small>{approval.approved_at}</small>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
```

---

## Policy Compliance

### Build & Release Ruleset (#15)

The promotion workflow enforces the following policies:

1. **Gate Validation:** All promotions must pass validation gates
2. **Approval Required:** At least one approval signature required
3. **Audit Trail:** All actions logged in `audit_logs` table
4. **Rollback Capability:** Previous version always preserved
5. **Ed25519 Signatures:** All approvals cryptographically signed

### Determinism Ruleset (#2)

Enforced via `determinism_check` gate:
- Max epsilon < 1e-6 (verified from golden run archive)
- Epsilon statistics must be available
- HKDF-derived seeds verified

### Artifacts Ruleset (#13)

Enforced via `hash_validation` gate:
- BLAKE3 bundle hash validated
- All adapter hashes present and non-empty
- SBOM verified (future integration)

---

## Error Handling

### Common Error Codes

| Code | HTTP Status | Description | Resolution |
|------|-------------|-------------|------------|
| `NOT_FOUND` | 404 | Golden run or promotion not found | Check run ID spelling |
| `BAD_REQUEST` | 400 | Invalid request parameters | Review request body schema |
| `FORBIDDEN` | 403 | Insufficient permissions | Check user role and permissions |
| `INTERNAL_ERROR` | 500 | Database or system error | Check server logs, retry |

### Error Response Format

```json
{
  "message": "golden run not found",
  "code": "NOT_FOUND",
  "details": "run_id: my-run-001"
}
```

---

## Testing

### Manual Testing

```bash
# 1. Create golden run (via aosctl)
aosctl golden create --name test-run-001 --bundle /path/to/bundle.ndjson

# 2. Request promotion
curl -X POST http://localhost:3000/v1/golden/test-run-001/promote \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"target_stage":"staging"}'

# 3. Check gate status (poll until complete)
curl http://localhost:3000/v1/golden/test-run-001/gates \
  -H "Authorization: Bearer $TOKEN"

# 4. Approve promotion
curl -X POST http://localhost:3000/v1/golden/test-run-001/approve \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"action":"approve","message":"Approved for staging"}'

# 5. Verify stage updated
curl http://localhost:3000/v1/golden/test-run-001/promotion \
  -H "Authorization: Bearer $TOKEN"

# 6. Test rollback
curl -X POST http://localhost:3000/v1/golden/staging/rollback \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"reason":"Testing rollback procedure"}'
```

### Integration Tests

```rust
#[tokio::test]
async fn test_promotion_workflow() {
    let db = Db::new_in_memory().await.unwrap();
    let state = AppState { db, /* ... */ };

    // 1. Request promotion
    let req = PromoteRequest {
        target_stage: "staging".to_string(),
        notes: Some("Test promotion".to_string()),
    };
    let response = request_promotion(state.clone(), claims.clone(), "test-run".to_string(), Json(req))
        .await
        .unwrap();

    // 2. Check gates
    tokio::time::sleep(Duration::from_secs(2)).await; // Wait for async gates
    let gates = get_gate_status(state.clone(), claims.clone(), "test-run".to_string())
        .await
        .unwrap();
    assert!(gates.iter().all(|g| g.passed));

    // 3. Approve
    let approve_req = ApproveRequest {
        action: "approve".to_string(),
        message: "Test approval".to_string(),
    };
    let approval = approve_or_reject_promotion(state.clone(), claims.clone(), "test-run".to_string(), Json(approve_req))
        .await
        .unwrap();
    assert_eq!(approval.status, "approved");
}
```

---

## Migration Guide

### Applying the Migration

```bash
# 1. Sign the new migration
./scripts/sign_migrations.sh

# 2. Run migration
aosctl db migrate

# 3. Verify tables created
sqlite3 var/aos-cp.sqlite3 ".schema golden_run_promotion_requests"
```

### Data Migration (if needed)

If migrating from old CAB workflow:

```sql
-- Map old promotion_history to new schema
INSERT INTO golden_run_promotion_history (golden_run_id, action, target_stage, promoted_by, approval_signature, promoted_at)
SELECT cpid, status, 'production', 'system', approval_signature, promoted_at
FROM promotion_history
WHERE status = 'production';
```

---

## Future Enhancements

1. **Multi-Approver Workflow:** Require N-of-M approvals for production
2. **Scheduled Promotions:** Allow scheduling promotions for maintenance windows
3. **Canary Deployments:** Gradual rollout with traffic splitting
4. **Automated Rollback:** Auto-rollback on metric threshold breach
5. **Promotion Templates:** Pre-configured promotion workflows per environment
6. **Slack/Email Notifications:** Alert approvers when promotion ready
7. **Gate Plugins:** Allow custom gate implementations
8. **Promotion Analytics:** Dashboard showing promotion success rates, time-to-promote metrics

---

## References

- [CLAUDE.md - Policy Packs](../CLAUDE.md#policy-packs)
- [CLAUDE.md - RBAC](../CLAUDE.md#rbac-5-roles-20-permissions)
- [ARCHITECTURE_PATTERNS.md - Deterministic Execution](./ARCHITECTURE_PATTERNS.md)
- [Database Migration 0076](../migrations/0076_golden_run_promotions.sql)
- [Golden Run Handlers](../crates/adapteros-server-api/src/handlers/golden.rs)
- [Promotion Handlers](../crates/adapteros-server-api/src/handlers/promotion.rs)

---

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
