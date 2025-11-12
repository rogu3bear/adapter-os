# Frontend Changes Required Based on Backend Answers

**Purpose:** Actionable list of changes needed in frontend to match backend API  
**Date:** 2025-01-15  
**Status:** Ready for implementation

---

## Critical Type Mismatches

### 1. UserRole Type (CRITICAL)

**File:** `ui/src/api/types.ts:46`

**Current:**
```typescript
export type UserRole = 'admin' | 'operator' | 'sre' | 'compliance' | 'auditor' | 'viewer';
```

**Should be:**
```typescript
export type UserRole = 'admin' | 'operator' | 'compliance' | 'viewer';
```

**Reason:** Backend only supports 4 roles. 'sre' and 'auditor' don't exist in backend.

**Impact:** High - Any code checking for 'sre' or 'auditor' roles will never match.

**Files to update:**
- `ui/src/api/types.ts`
- `ui/src/lib/rbac.ts` (if it references these roles)
- `ui/src/data/role-guidance.ts` (has 'sre' and 'auditor' entries)
- `ui/src/components/Dashboard.tsx` (has 'sre' and 'auditor' dashboard layouts)
- `ui/src/components/WorkflowWizard.tsx` (has 'sre' and 'auditor' workflows)
- `ui/src/components/ContextualHelp.tsx` (has 'sre' and 'auditor' help content)
- Any components checking for 'sre' or 'auditor' roles

**Action:** 
1. Remove 'sre' and 'auditor' from UserRole type
2. Map 'sre' → 'operator' (closest match)
3. Map 'auditor' → 'compliance' (closest match)
4. Update all role checks to use valid roles
5. Remove or consolidate 'sre'/'auditor' specific UI elements

---

### 2. SystemMetrics Type - Missing Fields

**File:** `ui/src/api/types.ts:693-728`

**Missing fields from backend:**
```typescript
export interface SystemMetrics {
  // Existing fields...
  
  // ADD THESE:
  cpu_usage: number;              // Required (0-100)
  memory_usage: number;           // Required (0-100)
  active_workers: number;         // Required
  requests_per_second: number;     // Required
  avg_latency_ms: number;          // Required
  disk_usage: number;             // Required (0-100)
  network_bandwidth: number;       // Required
  gpu_utilization: number;        // Required (0-100)
  uptime_seconds: number;         // Required
  process_count: number;           // Required
  load_average: {                  // Required
    load_1min: number;
    load_5min: number;
    load_15min: number;
  };
  timestamp: number;              // Required (Unix timestamp)
}
```

**Reason:** Backend returns these fields but frontend type is incomplete.

---

### 3. TrainingJob Type - Missing Fields

**File:** `ui/src/api/types.ts:841-875`

**Missing fields:**
```typescript
export interface TrainingJob {
  // Existing fields...
  
  // ADD:
  started_at?: string;            // ISO timestamp
  completed_at?: string;           // ISO timestamp
  estimated_completion?: string;   // ISO timestamp
  artifact_path?: string;          // Path to artifact file
  weights_hash_b3?: string;        // BLAKE3 hash of weights
}
```

---

### 4. AdapterResponse Type - Missing Fields

**File:** `ui/src/api/types.ts:405-435`

**Missing fields:**
```typescript
export interface Adapter {
  // Existing fields...
  
  // ADD:
  framework_id?: string;
  framework_version?: string;
  repo_id?: string;
  commit_sha?: string;
  intent?: string;
  current_state: AdapterState;     // Should be required, not optional
  pinned: boolean;                 // Should be required
  memory_bytes?: number;            // Memory usage in bytes
  last_activated?: string;         // ISO timestamp
  activation_count: number;         // Should be required
}
```

---

## SSE Endpoint URL Fixes

### 5. Metrics SSE Endpoint

**File:** `ui/src/api/client.ts:1636-1638`

**Current:**
```typescript
const sseUrl = import.meta.env.VITE_SSE_URL
  ? `ws://${import.meta.env.VITE_SSE_URL}/metrics`
  : `${import.meta.env.VITE_API_URL}/stream/metrics`;
```

**Should be:**
```typescript
const sseUrl = import.meta.env.VITE_SSE_URL
  ? `http://${import.meta.env.VITE_SSE_URL}/v1/stream/metrics`
  : `${this.baseUrl}/v1/stream/metrics`;
```

**Issues:**
- Wrong protocol (ws:// should be http:// for SSE)
- Wrong path (`/metrics` should be `/v1/stream/metrics`)
- Should use `this.baseUrl` instead of `import.meta.env.VITE_API_URL`
- Missing `/v1` prefix in path

**Backend endpoint:** `GET /v1/stream/metrics`

---

### 6. Activity Feed SSE Endpoint

**File:** `ui/src/api/client.ts` (subscribeToActivity method)

**Missing:** Need to add `subscribeToActivity` method that uses:
- Endpoint: `/v1/telemetry/events/recent/stream`
- Event type: `"activity"`
- Query params: `event_types[]` and `limit`

**Current state:** Method may exist but needs verification against backend spec.

---

## Missing API Client Methods

### 7. Log File Endpoints

**Missing methods:**
```typescript
async listLogFiles(): Promise<types.LogFileInfo[]>
async getLogFileContent(filename: string, params?: { lines?: number; offset?: number }): Promise<types.LogFileContentResponse>
subscribeToLogFile(filename: string, callback: (line: string) => void): () => void
```

**Backend endpoints:**
- `GET /v1/logs/files`
- `GET /v1/logs/files/:filename`
- `GET /v1/logs/files/:filename/stream`

---

### 8. Training Logs Endpoint

**File:** `ui/src/api/client.ts`

**Missing method:**
```typescript
async getTrainingLogs(jobId: string): Promise<string[]>
```

**Backend endpoint:** `GET /v1/training/jobs/:job_id/logs`

---

### 9. Adapter Bulk Operations

**File:** `ui/src/api/client.ts`

**Missing method:**
```typescript
async bulkAdapterLoad(request: types.BulkAdapterRequest): Promise<types.BulkAdapterResponse>
```

**Backend endpoint:** `POST /v1/adapters/bulk-load`

---

### 10. Model Import Status

**File:** `ui/src/api/client.ts`

**Missing method:**
```typescript
async getModelImportStatus(importId: string): Promise<types.ModelImportStatus>
```

**Backend endpoint:** `GET /v1/models/imports/:import_id`

---

## Error Handling Updates

### 11. Error Code Mapping

**File:** `ui/src/utils/errorMessages.ts`

**Add missing error codes:**
- `RATE_LIMIT_EXCEEDED` (429) - Currently only has `RATE_LIMITED`
- `DB_ERROR` / `DATABASE_ERROR` (500)
- `IO_ERROR` (500)
- `CRYPTO_ERROR` (500)
- `POLICY_VIOLATION` (403)
- `LOAD_FAILED` (500)
- `CONFIG_ERROR` (500)
- `TOKEN_ERROR` (500)

**Update existing:** `RATE_LIMITED` → `RATE_LIMIT_EXCEEDED` (backend uses latter)

---

### 12. Retry Logic for Rate Limits

**File:** `ui/src/utils/retry.ts` or `ui/src/api/client.ts`

**Add:** Rate limit errors (429) should NOT be retried immediately. Should:
- Extract `retry_after` from error details if present
- Wait for `retry_after` seconds before retry
- Otherwise, use exponential backoff

---

## Type Definition Updates

### 13. Claims Structure (JWT)

**File:** `ui/src/api/types.ts`

**Add:**
```typescript
export interface JWTClaims {
  sub: string;        // user_id
  email: string;
  role: string;       // 'admin' | 'operator' | 'compliance' | 'viewer'
  tenant_id: string;
  exp: number;        // Unix timestamp
  iat: number;        // Unix timestamp
  jti: string;        // JWT ID
  nbf: number;        // Not before timestamp
}
```

**Usage:** For parsing JWT tokens in frontend (if needed for debugging/display)

---

### 14. Policy JSON Schema Types

**File:** `ui/src/api/types.ts`

**Add complete policy pack types:**
```typescript
export interface PolicyJSON {
  schema: string;  // "adapteros.policy.v1"
  packs: {
    egress: EgressPolicyConfig;
    determinism: DeterminismPolicyConfig;
    router: RouterPolicyConfig;
    evidence: EvidencePolicyConfig;
    // ... all 22 policy packs
  };
}
```

**Reason:** Frontend policy editor needs complete type definitions.

---

### 15. Rate Limit Config Type

**File:** `ui/src/api/types.ts`

**Add:**
```typescript
export interface RateLimitConfig {
  requests_per_minute: number;  // Default: 1000
  burst_size: number;           // Default: 100
  inference_per_minute: number;  // Default: 50
}
```

---

## API Endpoint Path Corrections

### 16. Status Endpoint

**File:** `ui/src/api/client.ts`

**Current:** May be using `/v1/status`  
**Backend:** Uses `/v1/status` (verify this matches)

**Check:** Ensure status endpoint returns `AdapterOSStatus` structure matching backend.

---

### 17. Refresh Token Endpoint

**File:** `ui/src/api/client.ts:302`

**Current:** `refreshSession()` method exists  
**Verify:** Uses `POST /v1/auth/refresh` and handles cookie update

**Expected response:**
```typescript
{
  message: "token refreshed",
  user_id: string,
  role: string,
  tenant_id: string
}
```

**Note:** Cookie is updated automatically by backend (httpOnly).

---

## SSE Reconnection Improvements

### 18. SSE Keep-Alive Handling

**File:** `ui/src/api/client.ts` (all SSE methods)

**Current:** Reconnects on any error  
**Should:** 
- Ignore keep-alive events (empty data)
- Only reconnect on actual connection errors
- Handle `event: keepalive` events gracefully

---

### 19. SSE Event Type Filtering

**File:** `ui/src/api/client.ts:1735` (subscribeToMetrics)

**Current:** Listens for `'metrics'` event  
**Verify:** Backend sends `event: metrics` (correct)

**For activity stream:** Should listen for `event: activity`

---

## Query Parameter Fixes

### 20. Telemetry Events Query Params

**File:** `ui/src/api/client.ts:1608-1617`

**Current:** Uses `event_types[]` (correct)  
**Verify:** Backend accepts array format correctly

**Note:** Backend expects `event_types[]` in query string (multiple params with same name).

---

### 21. Adapter List Query Params

**File:** `ui/src/api/client.ts:396-402`

**Current:** Supports `tier` and `framework` (correct)  
**Verify:** Backend accepts these correctly

**Note:** No pagination support - returns all matching adapters.

---

## Missing Type Definitions

### 22. LogFileInfo Type

**File:** `ui/src/api/types.ts`

**Add:**
```typescript
export interface LogFileInfo {
  name: string;
  path: string;
  size_bytes: number;
  modified_at: string;  // ISO timestamp
  created_at: string;   // ISO timestamp
}

export interface ListLogFilesResponse {
  files: LogFileInfo[];
  total_size_bytes: number;
  count: number;
}

export interface LogFileContentResponse {
  filename: string;
  content: string[];      // Array of log lines
  total_lines: number;
  size_bytes: number;
  modified_at: string;    // ISO timestamp
}
```

---

### 23. BulkAdapterRequest/Response Types

**File:** `ui/src/api/types.ts`

**Add:**
```typescript
export interface BulkAdapterRequest {
  add: string[];          // Adapter IDs to load
  remove: string[];       // Adapter IDs to unload
  tenant_id?: string;     // Optional tenant ID
}

export interface BulkAdapterResponse {
  added: number;          // Number successfully added
  removed: number;        // Number successfully removed
  errors: string[];       // Array of error messages
}
```

---

### 24. Model Import Status Type

**File:** `ui/src/api/types.ts`

**Add:**
```typescript
export interface ModelImportStatus {
  import_id: string;
  model_id?: string;
  status: 'pending' | 'importing' | 'completed' | 'failed';
  progress_pct?: number;
  error_message?: string;
  created_at: string;
  completed_at?: string;
}
```

---

## Component Updates Needed

### 25. Role-Based UI Components

**Files:** All components using role checks

**Update:** Remove references to 'sre' and 'auditor' roles:
- Search for: `role === 'sre'` or `role === 'auditor'`
- Replace with: Appropriate role from 4 valid roles
- Or: Remove UI elements that were specific to these roles

---

### 26. Dashboard Recent Activity

**File:** `ui/src/components/Dashboard.tsx` (or similar)

**Current:** May be using mock data  
**Update:** Use `getRecentActivityEvents()` and `subscribeToActivity()` SSE

**Backend endpoints:**
- `GET /v1/telemetry/events/recent` (REST)
- `GET /v1/telemetry/events/recent/stream` (SSE)

---

### 27. Training Logs Display

**File:** `ui/src/components/TrainingMonitor.tsx` (or similar)

**Update:** Use `getTrainingLogs(jobId)` to display log file content

**Format:** Array of strings, each line: `[ISO_TIMESTAMP] {message}`

---

## Configuration Updates

### 28. API Base URL

**File:** `ui/src/api/client.ts:16`

**Current:**
```typescript
const API_BASE_URL = (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL || '/api';
```

**Verify:** Should be `/api` for same-origin, or full URL for CORS

**Note:** Backend serves API at `/api/v1/...` or `/v1/...` depending on proxy config.

---

## Testing Updates

### 29. Integration Test Updates

**Files:** `ui/src/**/*.test.ts` or `tests/ui/**/*.ts`

**Update:** All tests to use correct:
- Role names (4 roles only)
- Error codes (`RATE_LIMIT_EXCEEDED` not `RATE_LIMITED`)
- API endpoint paths
- Response structures

---

## Summary of Changes Required

### Critical (Must Fix Immediately)
1. ✅ **UserRole type** - Remove 'sre' and 'auditor' (185 references found)
2. ✅ **Metrics SSE URL** - Fix protocol and path
3. ✅ **SystemMetrics type** - Add missing required fields
4. ✅ **TrainingJob type** - Add missing fields
5. ✅ **Adapter type** - Add missing fields

### High Priority (Feature Completeness)
6. ✅ **SSE endpoints** - Fix all SSE URLs to use correct paths
7. ✅ **Error codes** - Update RATE_LIMITED → RATE_LIMIT_EXCEEDED
8. ✅ **Missing API methods** - Add log files, training logs, bulk operations
9. ✅ **Type definitions** - Add all missing types from backend

### Medium Priority (Polish)
10. ✅ **Role-based UI** - Update all components using 'sre'/'auditor'
11. ✅ **SSE reconnection** - Improve keep-alive handling
12. ✅ **Policy types** - Add complete policy schema types

### Files Requiring Changes

**Type Definitions:**
- `ui/src/api/types.ts` - Major updates needed

**API Client:**
- `ui/src/api/client.ts` - SSE fixes, new methods

**Components (Role Updates):**
- `ui/src/components/Dashboard.tsx` - Remove 'sre'/'auditor' layouts
- `ui/src/components/WorkflowWizard.tsx` - Remove 'sre'/'auditor' workflows  
- `ui/src/components/ContextualHelp.tsx` - Remove 'sre'/'auditor' help
- `ui/src/data/role-guidance.ts` - Remove 'sre'/'auditor' entries
- `ui/src/lib/rbac.ts` - Update role checks

**Error Handling:**
- `ui/src/utils/errorMessages.ts` - Add missing error codes

**Total Files:** ~15-20 files need updates

---

## Verification Checklist

After making changes, verify:

- [ ] All 4 roles work correctly (admin, operator, compliance, viewer)
- [ ] SSE streams connect and receive data
- [ ] Error messages display correctly for all error codes
- [ ] Training logs display correctly
- [ ] Adapter operations work (load, unload, pin, etc.)
- [ ] Metrics display all required fields
- [ ] Rate limiting errors handled gracefully
- [ ] Token refresh updates cookie correctly
- [ ] Activity feed displays recent events
- [ ] All API endpoints match backend routes

---

## Files to Modify

1. `ui/src/api/types.ts` - Type definitions
2. `ui/src/api/client.ts` - API client methods
3. `ui/src/utils/errorMessages.ts` - Error code mappings
4. `ui/src/lib/rbac.ts` - Role definitions
5. All component files checking roles
6. Test files

---

## Estimated Changes

- **Type definitions:** ~15 new types, ~10 type updates
- **API methods:** ~8 new methods, ~5 method fixes
- **SSE subscriptions:** ~3 fixes, ~1 new subscription
- **Error handling:** ~8 new error codes
- **Components:** ~10-15 files need role updates

**Total:** ~50-60 file changes

