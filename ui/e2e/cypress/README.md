# Cypress API Test Execution Guide

## How to Run the Tests

### 1. Interactive Mode (GUI)
```bash
cd ui
pnpm cypress:open
```
- Opens Cypress Test Runner GUI
- Select test files to run
- See real-time results and debug

### 2. Headless Mode (CI/CLI)
```bash
cd ui
pnpm cypress:run
```
- Runs all tests headlessly
- Suitable for CI/CD pipelines
- Generates reports

### 3. Run Specific Test File
```bash
cd ui
pnpm cypress:run --spec "e2e/cypress/e2e/api/auth.cy.ts"
```

### 4. Run with Environment Variables
```bash
cd ui
CYPRESS_API_BASE_URL=http://localhost:8080 \
CYPRESS_TEST_USER_EMAIL=admin@example.com \
CYPRESS_TEST_USER_PASSWORD=password \
pnpm cypress:run
```

---

## Different API Patterns to Test

### 1. Authentication Patterns

#### Public Endpoints (No Auth)
- `/healthz` - Health check
- `/readyz` - Readiness check  
- `/v1/meta` - API metadata
- `/v1/auth/login` - Login endpoint

**Test File:** `health.cy.ts` ✅

#### JWT-Protected Endpoints
- Most `/v1/*` endpoints
- Require `Authorization: Bearer <token>` header

**Test Files:** All API test files ✅

// Dual-auth OpenAI-compatible endpoints have been removed; no cloud-facing compat tests remain.

#### Metrics Endpoint (Bearer Token)
- `/metrics` - Prometheus metrics
- Requires metrics bearer token (different from JWT)

**Test File:** `health.cy.ts` (partial) ⚠️

#### Dev Bypass (Development Mode Only)
- `/v1/auth/dev-bypass` - Development token

**Test File:** `auth.cy.ts` (partial) ⚠️

---

### 2. Request/Response Patterns

#### Standard REST (JSON)
- Most endpoints use JSON request/response
- Content-Type: `application/json`

**Coverage:** ✅ All test files

#### SSE Streaming Endpoints
- `/v1/stream/metrics` - Real-time metrics
- `/v1/stream/telemetry` - Telemetry events
- `/v1/stream/adapters` - Adapter state changes
- `/v1/streams/training` - Training progress
- `/v1/telemetry/events/recent/stream` - Activity feed
- `/v1/monitoring/alerts/stream` - Alert stream
- `/v1/infer/stream` - Inference streaming

**Coverage:** ❌ Not yet tested (requires SSE handling)

#### File Upload Endpoints
- `/v1/adapters/import` - Adapter file upload
- Uses `multipart/form-data`

**Coverage:** ⚠️ Partial (may need FormData handling)

#### Query Parameter Auth
- SSE endpoints may accept `?token=xxx` for auth
- Some endpoints use query params for filtering

**Coverage:** ⚠️ Partial

---

### 3. Role-Based Access Patterns

Different endpoints require different roles:
- **admin** - Full access
- **operator** - Worker/plan management
- **compliance** - Audit/policy access
- **viewer** - Read-only

**Coverage:** ⚠️ Not yet tested (all tests use same user)

---

## Next Steps for Comprehensive Coverage

### Priority 1: SSE Streaming Tests

**Why:** Many endpoints use Server-Sent Events, need different testing approach.

**Implementation:**
```typescript
// Example SSE test pattern
it('should stream telemetry events', () => {
  cy.apiRequest({
    method: 'GET',
    url: '/v1/telemetry/events/recent/stream',
  }).then((response) => {
    // Handle SSE stream
    // Cypress doesn't natively support SSE, need workaround
  });
});
```

**Options:**
1. Use `cy.request()` with manual SSE parsing
2. Use Cypress plugin for SSE support
3. Test SSE endpoints separately with Node.js script

---

### Priority 2: Role-Based Testing

**Why:** Different endpoints require different permissions.

**Implementation:**
- Create test users with different roles
- Test endpoint access per role
- Verify 403 Forbidden for unauthorized roles

**Test Structure:**
```typescript
describe('Role-Based Access', () => {
  ['admin', 'operator', 'viewer'].forEach(role => {
    describe(`As ${role}`, () => {
      beforeEach(() => {
        cy.loginAs(role);
      });
      // Test endpoints accessible to this role
    });
  });
});
```

---

### Priority 3: File Upload Tests

**Why:** Adapter import uses file uploads.

**Implementation:**
```typescript
it('should upload adapter file', () => {
  cy.fixture('test-adapter.aos', 'binary').then(fileContent => {
    const blob = Cypress.Blob.binaryStringToBlob(fileContent);
    const formData = new FormData();
    formData.append('file', blob, 'test-adapter.aos');
    
    cy.apiRequest({
      method: 'POST',
      url: '/v1/adapters/import',
      body: formData,
      headers: {
        // Don't set Content-Type, let browser set it with boundary
      }
    });
  });
});
```

---

### Priority 4: Error Scenario Coverage

**Current:** Basic error validation exists.

**Enhance:**
- Test all error codes (400, 401, 403, 404, 422, 500, 503)
- Test rate limiting (429)
- Test validation errors
- Test timeout scenarios

---

### Priority 5: Integration Test Scenarios

**Current:** Individual endpoint tests.

**Add:**
- Complete workflows (create → use → delete)
- Multi-step operations (plan build → promote → deploy)
- Cross-endpoint dependencies

---

## Recommended Test Execution Order

### 1. Quick Smoke Test
```bash
# Test public endpoints first
pnpm cypress:run --spec "e2e/cypress/e2e/api/health.cy.ts"
```

### 2. Authentication Flow
```bash
# Verify auth works
pnpm cypress:run --spec "e2e/cypress/e2e/api/auth.cy.ts"
```

### 3. Core Functionality
```bash
# Test main API endpoints
pnpm cypress:run --spec "e2e/cypress/e2e/api/adapters.cy.ts"
pnpm cypress:run --spec "e2e/cypress/e2e/api/tenants.cy.ts"
```

### 4. Full Suite
```bash
# Run all API tests
pnpm cypress:run --spec "e2e/cypress/e2e/api/**/*.cy.ts"
```

---

## Missing Test Coverage

### Not Yet Covered:
1. **SSE Streaming** - Need SSE test utilities
2. **Role-Based Access** - Need multi-user test setup
3. **File Uploads** - Need FormData handling
4. **Rate Limiting** - Need rate limit testing
5. **Error Edge Cases** - Need comprehensive error scenarios
6. **Workflow Tests** - Need multi-step integration tests

### Partially Covered:
1. **Metrics Endpoint** - Basic test exists, needs bearer token config
2. **Dev Bypass** - Basic test exists, needs dev mode config
3. **OpenAI Compatibility** - Removed (no longer supported)

---

## Environment Setup

### Required:
- Backend server running on `http://localhost:8080`
- Test user created in database
- Database migrations applied

### Optional:
- Metrics bearer token configured (for `/metrics` tests)
- Dev mode enabled (for dev-bypass tests)
- OpenAI API key (no longer used)

---

## Quick Start

```bash
# 1. Start backend server (in another terminal)
cargo run --bin adapteros-server-api

# 2. Run Cypress tests
cd ui
pnpm cypress:run

# 3. Or open GUI for interactive testing
pnpm cypress:open
```

---

## Deterministic seed fixtures (UI e2e)

- Reset and seed: `cargo run -p adapteros-cli -- db seed-fixtures` (defaults: tenant `tenant-test`, model `model-qwen-test`, adapter `adapter-test`, stack `stack-test`, chat session `chat-session-test`)
- Skip reset: add `--skip-reset` (idempotent upserts), omit chat seed with `--no-chat`
- Cypress task: `cy.seedTestData({ skipReset: true })` uses the same command under the hood
- Tests now run against the live API (no cy.intercept stubs); ensure backend is running with `AOS_DEV_NO_AUTH=1 AOS_DEV_JWT_SECRET=test AOS_DETERMINISTIC=1` and a worker/model if inference is exercised.
- Seeded artifacts live in `cypress/fixtures/adapters/` (for uploads if needed); API fixtures remain for reference but are not intercepted.
- Env hints: `CYPRESS_TEST_TENANT_ID`, `CYPRESS_TEST_MODEL_ID`, `CYPRESS_TEST_ADAPTER_ID`, `CYPRESS_TEST_STACK_ID`, `CYPRESS_TEST_CHAT_SESSION_ID`

## Live no-stub UI e2e prerequisites

- Backend: running on `http://localhost:8080` with `AOS_DEV_NO_AUTH=1` (dev only).
- Worker: running with a tiny model (`config.json` + `model.safetensors`) pointed to `AOS_E2E_MODEL_PATH` and bound to a UDS (e.g., `AOS_E2E_UDS=/tmp/aos-e2e.sock`). Pre-clean stale sockets before runs.
- Run spec headless: `pnpm cypress:run --spec e2e/cypress/e2e/ui/adapter-chat.cy.ts` (consider a 5-minute watchdog in CI).
- The `db:seed-fixtures` Cypress task seeds deterministic IDs; no stubs are used, so live API responses must be available.

MLNavigator Inc 2025-12-09.
