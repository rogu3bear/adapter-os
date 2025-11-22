# Promotion Workflow API - Quick Reference

**For Frontend Developers**

## Base URL
```
http://localhost:3000  (dev)
https://api.adapteros.dev  (prod)
```

## Authentication
All endpoints require JWT token:
```typescript
headers: {
  'Authorization': `Bearer ${token}`,
  'Content-Type': 'application/json'
}
```

---

## API Endpoints

### 1. Request Promotion
```http
POST /v1/golden/:runId/promote
```
```typescript
// Request
{
  "target_stage": "staging" | "production",
  "notes": "Optional message"
}

// Response 200
{
  "request_id": "promo-xxx",
  "golden_run_id": "my-run",
  "target_stage": "staging",
  "status": "pending",
  "created_at": "2025-11-19T10:30:00Z"
}
```

### 2. Get Promotion Status
```http
GET /v1/golden/:runId/promotion
```
```typescript
// Response 200
{
  "request_id": "promo-xxx",
  "golden_run_id": "my-run",
  "target_stage": "staging",
  "status": "pending" | "approved" | "rejected" | "promoted",
  "requester_email": "user@example.com",
  "created_at": "...",
  "updated_at": "...",
  "notes": "...",
  "gates": [
    {
      "gate_name": "hash_validation",
      "status": "passed" | "failed" | "pending",
      "passed": true,
      "details": { /* JSON */ },
      "error_message": null,
      "checked_at": "..."
    }
  ],
  "approvals": [
    {
      "approver_email": "admin@example.com",
      "action": "approve" | "reject",
      "message": "...",
      "signature": "sig-xxx",
      "approved_at": "..."
    }
  ]
}
```

### 3. Approve/Reject
```http
POST /v1/golden/:runId/approve
```
```typescript
// Request
{
  "action": "approve" | "reject",
  "message": "Approval reason"
}

// Response 200
{
  "request_id": "promo-xxx",
  "status": "approved" | "rejected",
  "signature": "sig-xxx"
}
```

### 4. Get Gates Only
```http
GET /v1/golden/:runId/gates
```
```typescript
// Response 200
[
  {
    "gate_name": "hash_validation",
    "status": "passed",
    "passed": true,
    "details": { ... },
    "error_message": null,
    "checked_at": "..."
  }
]
```

### 5. Rollback
```http
POST /v1/golden/:stage/rollback
```
```typescript
// Request
{
  "reason": "Critical bug found"
}

// Response 200
{
  "stage": "production",
  "rolled_back_to": "previous-run",
  "rolled_back_from": "current-run",
  "reason": "..."
}
```

---

## React Hooks

```typescript
// Request promotion
const { mutate: requestPromotion } = useMutation({
  mutationFn: async ({ runId, targetStage, notes }) => {
    const res = await fetch(`/v1/golden/${runId}/promote`, {
      method: 'POST',
      headers: { 'Authorization': `Bearer ${token}`, 'Content-Type': 'application/json' },
      body: JSON.stringify({ target_stage: targetStage, notes })
    });
    return res.json();
  }
});

// Poll status
const { data: status } = useQuery({
  queryKey: ['promotion', runId],
  queryFn: async () => {
    const res = await fetch(`/v1/golden/${runId}/promotion`, {
      headers: { 'Authorization': `Bearer ${token}` }
    });
    return res.json();
  },
  refetchInterval: 2000 // Poll every 2s
});

// Approve/reject
const { mutate: approve } = useMutation({
  mutationFn: async ({ runId, action, message }) => {
    const res = await fetch(`/v1/golden/${runId}/approve`, {
      method: 'POST',
      headers: { 'Authorization': `Bearer ${token}`, 'Content-Type': 'application/json' },
      body: JSON.stringify({ action, message })
    });
    return res.json();
  }
});

// Rollback
const { mutate: rollback } = useMutation({
  mutationFn: async ({ stage, reason }) => {
    const res = await fetch(`/v1/golden/${stage}/rollback`, {
      method: 'POST',
      headers: { 'Authorization': `Bearer ${token}`, 'Content-Type': 'application/json' },
      body: JSON.stringify({ reason })
    });
    return res.json();
  }
});
```

---

## UI Flow

```
1. List golden runs (existing endpoint)
   ↓
2. User clicks "Promote" → Request promotion (POST /promote)
   ↓
3. Show status panel → Poll status (GET /promotion) every 2s
   ↓
4. Display gates: hash_validation, policy_check, determinism_check
   ↓
5. When all gates pass → Enable "Approve" button
   ↓
6. User clicks "Approve" → POST /approve with action="approve"
   ↓
7. Status changes to "promoted" → Show success message
```

---

## Gate Status

| Gate Name | Purpose | Pass Criteria |
|-----------|---------|---------------|
| `hash_validation` | Verify bundle integrity | Bundle hash exists, adapters present |
| `policy_check` | Policy compliance | All 23 policies pass |
| `determinism_check` | Epsilon validation | max_epsilon < 1e-6 |

---

## Error Codes

| Code | Status | Meaning |
|------|--------|---------|
| `NOT_FOUND` | 404 | Golden run or promotion not found |
| `BAD_REQUEST` | 400 | Invalid request (wrong stage, already processed) |
| `FORBIDDEN` | 403 | Insufficient permissions |
| `INTERNAL_ERROR` | 500 | Server error |

---

## Example Component

```tsx
import { useQuery, useMutation } from '@tanstack/react-query';

export const PromotionPanel = ({ runId }: { runId: string }) => {
  const { data } = useQuery({
    queryKey: ['promotion', runId],
    queryFn: () => fetch(`/v1/golden/${runId}/promotion`, {
      headers: { 'Authorization': `Bearer ${token}` }
    }).then(r => r.json()),
    refetchInterval: 2000
  });

  const approve = useMutation({
    mutationFn: (action: 'approve' | 'reject') =>
      fetch(`/v1/golden/${runId}/approve`, {
        method: 'POST',
        headers: { 'Authorization': `Bearer ${token}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ action, message: 'Auto message' })
      }).then(r => r.json())
  });

  if (!data) return <div>Loading...</div>;

  const allPassed = data.gates.every(g => g.passed);

  return (
    <div>
      <h2>Status: {data.status}</h2>
      <div>
        {data.gates.map(gate => (
          <div key={gate.gate_name}>
            {gate.gate_name}: {gate.status}
            {gate.error_message && <span className="error">{gate.error_message}</span>}
          </div>
        ))}
      </div>
      {allPassed && data.status === 'pending' && (
        <button onClick={() => approve.mutate('approve')}>Approve</button>
      )}
    </div>
  );
};
```

---

## Full Documentation

See `/Users/star/Dev/aos/docs/PROMOTION_WORKFLOW_API.md` for complete details.

---

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
