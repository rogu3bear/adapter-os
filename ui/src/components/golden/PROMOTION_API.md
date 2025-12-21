# Promotion Workflow API Endpoints

**Component:** `/Users/star/Dev/aos/ui/src/components/golden/PromotionWorkflow.tsx`

**Agent:** Agent 3
**Date:** 2025-11-19

## Required API Endpoints

The PromotionWorkflow component requires the following backend API endpoints to be implemented:

### 1. Get Promotion Status

```
GET /v1/golden/:run_id/promotion-status
```

**Response:**
```typescript
{
  stages: Array<{
    id: string;
    name: string;
    description: string;
    status: 'pending' | 'in_progress' | 'passed' | 'failed' | 'skipped';
    approver?: string;
    approved_at?: string;  // ISO-8601 timestamp
    notes?: string;
    gates: Array<{
      id: string;
      name: string;
      description: string;
      status: 'pending' | 'passed' | 'failed';
      required: boolean;
      error_message?: string;
      last_checked?: string;  // ISO-8601 timestamp
    }>;
  }>;
}
```

### 2. Approve Promotion Stage

```
POST /v1/golden/:run_id/promotion/approve
```

**Request Body:**
```typescript
{
  stage_id: string;
  notes?: string;
  approver?: string;  // Optional, can be derived from JWT
}
```

**Response:**
```typescript
{
  success: boolean;
  stage_id: string;
  status: 'passed';
  approved_at: string;  // ISO-8601 timestamp
  approver: string;
}
```

### 3. Reject Promotion Stage

```
POST /v1/golden/:run_id/promotion/reject
```

**Request Body:**
```typescript
{
  stage_id: string;
  reason: string;  // Required
}
```

**Response:**
```typescript
{
  success: boolean;
  stage_id: string;
  status: 'failed';
  rejected_at: string;  // ISO-8601 timestamp
  reason: string;
}
```

### 4. Execute Promotion

```
POST /v1/golden/:run_id/promotion/execute
```

**Request Body:**
```typescript
{
  rollback_plan: {
    trigger_conditions: string[];
    rollback_steps: string[];
    notification_contacts: string[];
  };
}
```

**Response:**
```typescript
{
  success: boolean;
  execution_id: string;
  executed_at: string;  // ISO-8601 timestamp
  promoted_to: string;  // Target environment (e.g., 'production')
}
```

### 5. Rollback Promotion

```
POST /v1/golden/:run_id/promotion/rollback
```

**Request Body:**
```typescript
{
  reason?: string;
}
```

**Response:**
```typescript
{
  success: boolean;
  rollback_id: string;
  rolled_back_at: string;  // ISO-8601 timestamp
  previous_stage: string;
}
```

### 6. Check Gate Status

```
POST /v1/golden/:run_id/gates/check
```

**Request Body:**
```typescript
{
  stage_id: string;
  gate_ids?: string[];  // Optional, check specific gates only
}
```

**Response:**
```typescript
{
  gates: Array<{
    id: string;
    name: string;
    status: 'passed' | 'failed' | 'pending';
    checked_at: string;  // ISO-8601 timestamp
    error_message?: string;
  }>;
}
```

## Database Schema Suggestions

### `promotion_stages` table
```sql
CREATE TABLE promotion_stages (
  id TEXT PRIMARY KEY,
  golden_run_id TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  status TEXT NOT NULL CHECK (status IN ('pending', 'in_progress', 'passed', 'failed', 'skipped')),
  approver TEXT,
  approved_at TEXT,
  notes TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (golden_run_id) REFERENCES golden_runs(run_id)
);
```

### `promotion_gates` table
```sql
CREATE TABLE promotion_gates (
  id TEXT PRIMARY KEY,
  stage_id TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  status TEXT NOT NULL CHECK (status IN ('pending', 'passed', 'failed')),
  required BOOLEAN NOT NULL DEFAULT 1,
  error_message TEXT,
  last_checked TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (stage_id) REFERENCES promotion_stages(id)
);
```

### `promotion_rollback_plans` table
```sql
CREATE TABLE promotion_rollback_plans (
  id TEXT PRIMARY KEY,
  golden_run_id TEXT NOT NULL,
  trigger_conditions_json TEXT NOT NULL,  -- JSON array of strings
  rollback_steps_json TEXT NOT NULL,      -- JSON array of strings
  notification_contacts_json TEXT NOT NULL, -- JSON array of strings
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (golden_run_id) REFERENCES golden_runs(run_id)
);
```

### `promotion_executions` table
```sql
CREATE TABLE promotion_executions (
  id TEXT PRIMARY KEY,
  golden_run_id TEXT NOT NULL,
  execution_type TEXT NOT NULL CHECK (execution_type IN ('promote', 'rollback')),
  executed_at TEXT NOT NULL,
  executed_by TEXT NOT NULL,
  target_environment TEXT,
  rollback_plan_id TEXT,
  success BOOLEAN NOT NULL,
  error_message TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (golden_run_id) REFERENCES golden_runs(run_id),
  FOREIGN KEY (rollback_plan_id) REFERENCES promotion_rollback_plans(id)
);
```

## RBAC Considerations

**Required Permissions:**
- `PromotionApprove` - Approve promotion stages (Operator+, Admin)
- `PromotionReject` - Reject promotion stages (Operator+, Admin)
- `PromotionExecute` - Execute promotions to production (Admin only)
- `PromotionRollback` - Initiate rollbacks (Admin, SRE)
- `PromotionView` - View promotion status (All roles)

## Policy Compliance

This component implements:

1. **Audit Trail** - All promotion actions logged with user, timestamp, and reason
2. **Approval Gates** - Multi-stage approval workflow with required checks
3. **Rollback Planning** - Mandatory rollback procedures for production promotions
4. **Evidence Collection** - Gate checks provide evidence for compliance audits

## Integration Notes

1. The component uses mock data in `loadPromotionStatus()` - replace with actual API calls
2. All API calls are commented out with `// await apiClient.request(...)` patterns
3. Error handling uses the existing `logger` and `toast` patterns
4. RBAC checks should be performed on the backend for all mutation endpoints

## Testing Checklist

- [ ] GET /v1/golden/:run_id/promotion-status returns correct stage data
- [ ] POST /v1/golden/:run_id/promotion/approve advances workflow
- [ ] POST /v1/golden/:run_id/promotion/reject blocks progression
- [ ] POST /v1/golden/:run_id/promotion/execute requires rollback plan
- [ ] POST /v1/golden/:run_id/promotion/rollback reverts to previous stage
- [ ] Gate checks validate before approval
- [ ] Permissions enforced for approve/reject/execute/rollback
- [ ] Audit logs created for all promotion actions
