# AdapterOS UI → API Mapping

> Comprehensive mapping of Leptos UI pages to backend API dependencies.

## API Architecture

**Client:** `crates/adapteros-ui/src/api/client.rs` (~1,800 lines)

**Authentication:**
- Type: httpOnly cookie (secure, non-localStorage)
- CSRF: X-CSRF-Token header for mutations
- Credentials: `Include` mode for fetch

**Error Handling:**
- Centralized `ApiError` type
- All endpoints return `ApiResult<T>`

**SSE Support:**
- Module: `src/api/sse.rs`
- States: Disconnected → Connecting → Connected → Error → CircuitOpen
- Circuit breaker: 3 failures, exponential backoff (1s → 30s), 60s reset

---

## Page-by-Page Mapping

### Login Page
**File:** `src/pages/login.rs`

| Component | Endpoint | Method | Notes |
|-----------|----------|--------|-------|
| Login Form | `/v1/auth/login` | POST | httpOnly cookie set |

---

### Dashboard
**File:** `src/pages/dashboard.rs`

| Component | Endpoint | Method | Type |
|-----------|----------|--------|------|
| System Status | `/v1/system/status` | GET | One-time |
| Workers List | `/v1/workers` | GET | One-time |
| Live Metrics | `/v1/stream/metrics` | SSE (`metrics` event) | Real-time |

**Features:**
- Metrics history (60 data points max)
- Sparklines for CPU, Memory, GPU, RPS, Latency
- SSE connection state indicator
- Dashboard listens to the `metrics` event type for live updates

---

### Chat Page
**File:** `src/pages/chat.rs`

| Component | Endpoint | Method | Type |
|-----------|----------|--------|------|
| Chat Session | `/v1/infer/stream` | POST | SSE Streaming |

**Request:**
```json
{
  "prompt": "string",
  "max_tokens": 1024,
  "temperature": 0.7,
  "adapters": ["optional", "ids"]
}
```

**SSE Events:**
- `Token { text }` - Individual token
- `Done { total_tokens, latency_ms, trace_id }` - End
- `Error { message }` - Error

**Features:**
- Real-time token streaming
- Cancellation via AbortSignal
- Trace ID capture
- Session management (`/chat/{session_id}`)

---

### Adapters Page
**File:** `src/pages/adapters.rs`

| Component | Endpoint | Method |
|-----------|----------|--------|
| Adapter List | `/v1/adapters` | GET |
| Adapter Detail | `/v1/adapters/{id}` | GET |

---

### Models Page
**File:** `src/pages/models.rs`

| Component | Endpoint | Method |
|-----------|----------|--------|
| Model List | `/v1/models` | GET |
| Model Status | `/v1/models/{id}/status` | GET |
| Import Model | `/v1/models/import` | POST |

---

### Training Page
**File:** `src/pages/training.rs`

| Component | Endpoint | Method | Polling |
|-----------|----------|--------|---------|
| Jobs List | `/v1/training/jobs` | GET | 5s |
| Job Detail | `/v1/training/jobs/{id}` | GET | 5s |
| Create Job | `/v1/training/jobs` | POST | - |
| Cancel Job | `/v1/training/jobs/{id}/cancel` | POST | - |
| Job Logs | `/v1/training/jobs/{id}/logs` | GET | - |

**Features:**
- Dual-panel UI (list + detail)
- Status filtering
- Live polling for updates

---

### Stacks Page
**File:** `src/pages/stacks.rs`

| Component | Endpoint | Method |
|-----------|----------|--------|
| List Stacks | `/v1/adapter-stacks` | GET |
| Get Stack | `/v1/adapter-stacks/{id}` | GET |
| Create Stack | `/v1/adapter-stacks` | POST |
| Update Stack | `/v1/adapter-stacks/{id}` | PUT |
| Delete Stack | `/v1/adapter-stacks/{id}` | DELETE |
| Activate Stack | `/v1/adapter-stacks/{id}/activate` | POST |
| Deactivate Stack | `/v1/adapter-stacks/deactivate` | POST |

---

### Documents Page
**File:** `src/pages/documents.rs`

| Component | Endpoint | Method | Notes |
|-----------|----------|--------|-------|
| Document List | `/v1/documents` | GET | Paginated |
| Document Detail | `/v1/documents/{id}` | GET | |
| Upload | `/v1/documents` | POST | multipart/form-data |
| Delete | `/v1/documents/{id}` | DELETE | |
| Chunks | `/v1/documents/{id}/chunks` | GET | |
| Process | `/v1/documents/{id}/process` | POST | |
| Retry Failed | `/v1/documents/{id}/retry` | POST | |
| List Failed | `/v1/documents/failed` | GET | |

**Features:**
- Raw Fetch API for uploads
- Drag-and-drop support
- Auto-deduplication

---

### Repositories Page
**File:** `src/pages/repositories.rs`

| Component | Endpoint | Method |
|-----------|----------|--------|
| List Repos | `/v1/repositories` | GET |
| Get Repo | `/v1/repositories/{id}` | GET |
| Register Repo | `/v1/repositories` | POST |
| Delete Repo | `/v1/repositories/{id}` | DELETE |
| Sync/Scan | `/v1/repositories/{id}/sync` | POST |
| Sync Status | `/v1/repositories/{id}/sync/status` | GET |
| Publish Adapter | `/v1/repositories/{id}/publish` | POST |

---

### Audit Page
**File:** `src/pages/audit.rs`

| Component | Endpoint | Method |
|-----------|----------|--------|
| Audit Logs | `/v1/audit/logs` | GET |
| Audit Chain | `/v1/audit/chain` | GET |
| Verify Chain | `/v1/audit/chain/verify` | GET |
| Compliance Audit | `/v1/audit/compliance` | GET |
| Federation Audit | `/v1/audit/federation` | GET |

**Tabs:**
1. Timeline - Raw events
2. Hash Chain - Blockchain-style linkage
3. Merkle Tree - Tree visualization
4. Compliance - Control status

---

### Collections Page
**File:** `src/pages/collections.rs`

| Component | Endpoint | Method |
|-----------|----------|--------|
| List | `/v1/collections` | GET |
| Get | `/v1/collections/{id}` | GET |
| Create | `/v1/collections` | POST |
| Delete | `/v1/collections/{id}` | DELETE |
| Add Document | `/v1/collections/{id}/documents` | POST |
| Remove Document | `/v1/collections/{id}/documents/{doc_id}` | DELETE |

---

### Policies Page
**File:** `src/pages/policies.rs`

| Component | Endpoint | Method |
|-----------|----------|--------|
| List Policies | `/v1/policies` | GET |
| Get Policy | `/v1/policies/{cpid}` | GET |

---

### Settings Page
**File:** `src/pages/settings.rs`

| Component | Endpoint | Method |
|-----------|----------|--------|
| Get Settings | `/v1/settings` | GET |
| Update Settings | `/v1/settings` | PUT |
| Current User | `/v1/auth/me` | GET |
| Logout | `/v1/auth/logout` | POST |

**Tabs:**
1. Profile - User info
2. API Configuration - URLs, timeouts
3. UI Preferences - Theme, defaults
4. System Info - Version, health

---

### Workers Page
**File:** `src/pages/workers.rs`

| Component | Endpoint | Method |
|-----------|----------|--------|
| List Workers | `/v1/workers` | GET |
| Get Worker | `/v1/workers/{id}` | GET |
| Spawn Worker | `/v1/workers/spawn` | POST |
| Drain Worker | `/v1/workers/{id}/drain` | POST |
| Stop Worker | `/v1/workers/{id}/stop` | POST |
| Worker Metrics | `/v1/workers/{id}/metrics` | GET |

---

### System Page
**File:** `src/pages/system.rs`

| Component | Endpoint | Method |
|-----------|----------|--------|
| System Status | `/v1/system/status` | GET |
| System Metrics | `/v1/metrics/system` | GET |
| List Nodes | `/v1/nodes` | GET |

---

### Diagnostics / Flight Recorder
**Files:** `src/pages/flight_recorder.rs`, `src/pages/diff.rs`

| Component | Endpoint | Method |
|-----------|----------|--------|
| List Diag Runs | `/v1/diag/runs` | GET |
| Diff Runs | `/v1/diag/diff` | POST |
| Export Run | `/v1/diag/runs/{id}/export` | GET |

---

### Global Search

| Component | Endpoint | Method |
|-----------|----------|--------|
| Search | `/v1/search` | GET |

**Params:** `q`, `scope`, `limit`

---

### Inference Traces

| Component | Endpoint | Method |
|-----------|----------|--------|
| Search Traces | `/v1/traces/search` | GET |
| Get Trace | `/v1/traces/{trace_id}` | GET |
| List Traces | `/v1/traces/inference` | GET |
| Trace Detail | `/v1/traces/inference/{id}` | GET |

---

### Health Endpoints

| Component | Endpoint | Method |
|-----------|----------|--------|
| Liveness | `/healthz` | GET |
| Readiness | `/readyz` | GET |

---

## API Hooks

### Core Hooks (`src/hooks/mod.rs`)

| Hook | Purpose |
|------|---------|
| `use_api_resource<T>()` | Wraps API call in LoadingState |
| `use_polling(interval, fn)` | Polls every N ms |
| `use_navigate()` | Router navigation |
| `use_api()` | ApiClient singleton |

### SSE Hooks (`src/hooks/use_sse_notifications.rs`)

| Hook | Purpose |
|------|---------|
| `use_sse()` | Raw SSE connection |
| `use_sse_json<T>()` | Typed SSE events |
| `use_sse_notifications()` | Toast notifications for SSE state |

---

## Authentication Flow

1. **Login** → POST `/v1/auth/login` → httpOnly cookie
2. **In-Memory** → `ApiClient.set_auth_status(true)`
3. **Requests** → Cookies auto-included
4. **Logout** → POST `/v1/auth/logout` → Clear state
5. **Protected Routes** → Redirect to `/login` if not authed

---

## Request Patterns

| Pattern | Method | Example |
|---------|--------|---------|
| Fetch List | GET | `/v1/adapters` |
| Fetch Single | GET | `/v1/adapters/{id}` |
| Create | POST | `/v1/adapters` |
| Update | PUT | `/v1/adapter-stacks/{id}` |
| Delete | DELETE | `/v1/documents/{id}` |
| Action | POST | `/v1/workers/{id}/drain` |
| Stream | POST/SSE | `/v1/infer/stream` |
| Metrics Stream | GET/SSE | `/v1/stream/metrics` |

---

## Summary

| Category | Count |
|----------|-------|
| REST Endpoints Used | 65+ |
| SSE Endpoints | 2 |
| Pages | 19 |
| API Client Methods | 70+ |
