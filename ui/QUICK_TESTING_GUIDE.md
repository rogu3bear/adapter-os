# Quick Testing Guide - UI Backend Integration

**Purpose:** Verify all 8 UI pages (U1-U8) are working with real backend APIs

**Time Required:** ~15-20 minutes

---

## Prerequisites

### 1. Start Backend Server

```bash
# From repository root
export AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-mlx
export DATABASE_URL=sqlite://var/aos-cp.sqlite3

# Start backend API server
cargo run --release -p adapteros-server-api
# Server should start on http://localhost:8080
```

### 2. Start UI Dev Server

```bash
# In a new terminal
cd ui
pnpm install  # First time only
pnpm dev
# UI should open at http://localhost:3200
```

### 3. Verify Proxy Configuration

Open browser DevTools (F12) → Network tab. All API calls should show as `/api/v1/...` and proxy to `http://localhost:8080`.

---

## Testing Checklist

### ✅ U1: Dashboard (`/`)

**Actions:**
1. Navigate to `http://localhost:3200/`
2. Check stats cards display numbers (nodes, tenants, adapters, performance)
3. Look for SSE connection indicator (green "Live" badge on CPU usage)
4. Watch metrics update in real-time (every few seconds)
5. Open DevTools → Network → Filter by "metrics"

**Expected API Calls:**
- `GET /api/v1/metrics/system` → 200 OK
- `GET /api/v1/stream/metrics` → 200 OK (EventSource)
- `GET /api/v1/nodes` → 200 OK
- `GET /api/v1/tenants` → 200 OK

**Pass Criteria:**
- [ ] Stats cards show real numbers (not 0 or placeholder)
- [ ] SSE "Live" badge visible with green dot
- [ ] No red error alerts
- [ ] Network tab shows successful API calls

---

### ✅ U2: Adapters List (`/adapters`)

**Actions:**
1. Navigate to `http://localhost:3200/adapters`
2. Check stats cards (Total, Loaded, Pinned, Memory Used)
3. Verify adapter table displays adapters (or "No adapters" if empty)
4. Open DevTools → Network → Filter by "adapters"

**Expected API Calls:**
- `GET /api/v1/adapters` → 200 OK (JSON array)

**Pass Criteria:**
- [ ] Stats cards show real numbers
- [ ] Adapter table loads (with data or empty state)
- [ ] "Refresh" button works without errors
- [ ] Network tab shows `GET /api/v1/adapters`

---

### ✅ U3: Adapter Detail (`/adapters/:id`)

**Actions:**
1. From adapters list, click on any adapter row
2. Detail page should load with tabs
3. Switch between tabs: Overview, Activations, Lineage, Manifest
4. Check for SSE connection status
5. Open DevTools → Network → Filter by "adapters"

**Expected API Calls:**
- `GET /api/v1/adapters/:id` → 200 OK
- `GET /api/v1/adapters/:id/lineage` → 200 OK
- `GET /api/v1/adapters/:id/activations` → 200 OK
- `GET /api/v1/adapters/:id/manifest` → 200 OK
- `GET /api/v1/stream/adapters` → 200 OK (EventSource)

**Pass Criteria:**
- [ ] Adapter details display (name, tier, state)
- [ ] All tabs load without errors
- [ ] Manifest tab shows JSON data
- [ ] Back button returns to adapter list

---

### ✅ U4: Training Jobs (`/training`)

**Actions:**
1. Navigate to `http://localhost:3200/training`
2. Check training jobs list (may be empty)
3. Click "Start Training" button (if permission available)
4. Open DevTools → Network → Filter by "training"

**Expected API Calls:**
- `GET /api/v1/training/jobs` → 200 OK (JSON array)

**Pass Criteria:**
- [ ] Jobs list loads (with data or empty state)
- [ ] "Start Training" button appears (if RBAC allows)
- [ ] Network tab shows `GET /api/v1/training/jobs`
- [ ] No console errors

---

### ✅ U5: Inference (`/inference`)

**Actions:**
1. Navigate to `http://localhost:3200/inference`
2. Check adapter dropdown (should load adapters)
3. Enter prompt: "Hello, how are you?"
4. Click "Generate" button
5. Wait for response to appear
6. Open DevTools → Network → Filter by "infer"

**Expected API Calls:**
- `GET /api/v1/adapters` → 200 OK (for dropdown)
- `POST /api/v1/infer` → 200 OK (standard mode)

**Pass Criteria:**
- [ ] Adapter dropdown populates with adapters
- [ ] "Generate" button enabled
- [ ] Response appears in output panel
- [ ] Token count and latency displayed
- [ ] Network tab shows `POST /api/v1/infer`

**Bonus: Test Streaming Mode**
1. Click "Streaming" mode button
2. Enter prompt and click "Generate"
3. Watch tokens appear one-by-one

---

### ✅ U6: Datasets (`/training` → Datasets Tab)

**Actions:**
1. Navigate to `http://localhost:3200/training`
2. Click "Datasets" tab
3. Check datasets list (may be empty)
4. Open DevTools → Network → Filter by "datasets"

**Expected API Calls:**
- `GET /api/v1/datasets` → 200 OK (JSON array)

**Pass Criteria:**
- [ ] Datasets list loads (with data or empty state)
- [ ] "Upload Dataset" button appears (if RBAC allows)
- [ ] Network tab shows `GET /api/v1/datasets`
- [ ] No console errors

---

### ✅ U7: Policies (`/policies`)

**Actions:**
1. Navigate to `http://localhost:3200/policies`
2. Check stats cards (Total, Active, Draft, Signed)
3. Verify policy list displays
4. Open DevTools → Network → Filter by "policies"

**Expected API Calls:**
- `GET /api/v1/policies` → 200 OK (JSON array)

**Pass Criteria:**
- [ ] Stats cards show real numbers
- [ ] Policy list loads with 23 canonical policies
- [ ] "Refresh" button works
- [ ] Network tab shows `GET /api/v1/policies`

---

### ✅ U8: System Metrics (`/metrics`)

**Actions:**
1. Navigate to `http://localhost:3200/metrics`
2. Wait for charts to populate (may take a few seconds)
3. Watch charts update in real-time
4. Check metric cards at bottom (CPU, Memory, GPU, etc.)
5. Open DevTools → Network → Filter by "metrics"

**Expected API Calls:**
- `GET /api/v1/metrics/system` → 200 OK (polling)
- `GET /api/v1/stream/metrics` → 200 OK (EventSource)

**Pass Criteria:**
- [ ] Charts render with data points
- [ ] Metric cards show real values (not "--")
- [ ] Charts update every few seconds
- [ ] "Last updated" badge shows current time
- [ ] Network tab shows both REST and SSE requests

---

## Common Issues & Fixes

### Issue: "Network Error" or 404 on API calls

**Fix:**
```bash
# Verify backend is running on port 8080
curl http://localhost:8080/healthz
# Should return: {"status":"ok"}

# Check Vite proxy is configured
cat ui/vite.config.ts | grep -A 5 proxy
# Should show: target: 'http://localhost:8080'
```

### Issue: No SSE connection ("Live" badge not showing)

**Fix:**
```bash
# Verify SSE endpoint works
curl -N http://localhost:8080/api/v1/stream/metrics
# Should stream events

# Check browser console for EventSource errors
# Open DevTools → Console → filter by "SSE" or "EventSource"
```

### Issue: Empty data (no adapters, no jobs, etc.)

**Fix:**
```bash
# Initialize database and create sample data
cd /path/to/adapter-os
./target/release/aosctl db migrate

# Create a tenant (required for most operations)
./target/release/aosctl init-tenant --id default --uid 1000 --gid 1000

# Register a sample adapter (optional)
./target/release/aosctl adapter register sample-adapter <hash>
```

### Issue: RBAC permission errors

**Fix:**
```bash
# Login as admin user (if auth is enabled)
# Navigate to: http://localhost:3200/auth/login

# Check user permissions in dashboard
# User role should be "admin" or "operator" for most actions
```

---

## Success Criteria

**All 8 pages should:**
- ✅ Load without console errors
- ✅ Display real data from backend APIs
- ✅ Show loading states while fetching
- ✅ Handle errors gracefully (with retry buttons)
- ✅ Make visible API calls in Network tab
- ✅ Update in real-time (where SSE is used)

**Total API Calls (across all pages):** ~15-20 unique endpoints

---

## Reporting Issues

If any test fails:
1. Note which page/test failed
2. Copy error message from console
3. Copy network request/response from DevTools
4. Screenshot the error UI
5. Report to Team 5 channel

---

## Next Steps After Testing

1. **Document Results:** Update `UI_BACKEND_INTEGRATION_STATUS.md` with test results
2. **Fix Any Issues:** Create tickets for bugs found
3. **Deploy to Staging:** Once all tests pass, deploy to staging environment
4. **E2E Tests:** Run Cypress E2E tests (if configured)
5. **Performance Testing:** Check page load times and API latency

---

**Testing Complete?** Mark all checkboxes above and report status to team lead.

**Time Spent:** ______ minutes
**Issues Found:** ______ (list below)
**Status:** ⬜ Pass / ⬜ Fail / ⬜ Needs Review
