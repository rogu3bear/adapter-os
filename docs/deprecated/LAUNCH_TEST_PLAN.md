# AdapterOS Launch Script Test Plan

**Date:** 2025-11-22
**Status:** Ready for Testing
**Prerequisites Verified:** ✅ All components present and aligned

---

## Prerequisites Verification

All infrastructure components are in place:

- ✅ `configs/cp.toml` exists (backend configuration)
- ✅ `scripts/graceful-shutdown.sh` exists (shutdown handler)
- ✅ `scripts/service-manager.sh` exists (service management)
- ✅ `scripts/launch.sh` exists with 8080 port configuration
- ✅ `ui/package.json` exists (UI setup ready)
- ✅ `lsof` and `pgrep` available (port/process management)

---

## Port Configuration

| Component | Port | Config Source | Status |
|-----------|------|---|--------|
| Backend API | 8080 | `configs/cp.toml:5` | ✅ Correct |
| Web UI | 3200 | `ui/vite.config.ts` | ✅ Correct |
| Metrics (Optional) | 9090 | N/A | Optional |

---

## Manual Test Procedure

### Phase 1: Environment Setup

**Objective:** Ensure ports are free and basic commands work

```bash
# Step 1: Check if ports are available
lsof -i :8080 && echo "⚠️  Port 8080 in use" || echo "✅ Port 8080 available"
lsof -i :3200 && echo "⚠️  Port 3200 in use" || echo "✅ Port 3200 available"

# Step 2: Verify database directory exists
mkdir -p var
ls -la var/ | head -5
# Expected: var/ directory with potential existing logs, adapters, bundles

# Step 3: Verify configuration is readable
cat configs/cp.toml | grep "^port = "
# Expected output: port = 8080

# Step 4: Verify binaries/dependencies can be built
cargo build -p adapteros-server 2>&1 | tail -5
# Expected: Compilation to complete or "already compiled" message

# Step 5: Check UI dependencies
cd ui && npm list 2>&1 | head -10
# Expected: node_modules present or indication pnpm can install
```

**Expected Results:**
- Both ports 8080 and 3200 are free
- `var/` directory created
- Port 8080 confirmed in config
- Backend compiles successfully
- UI dependencies available

---

### Phase 2: Launch Script Execution

**Objective:** Test the full system startup

```bash
# Step 1: Full system launch
cd /Users/star/Dev/aos
./launch.sh
# Expected output:
# - Banner displaying
# - Pre-flight checks passing
# - Services starting in order
# - Access URLs displayed (8080, 3200)
# - Periodic status checks every 30s

# Step 2: Monitor startup (in separate terminal while launch.sh runs)
# Terminal 2:
watch -n 2 'lsof -i :8080,:3200'
# Expected: PID for backend (port 8080) and UI (port 3200) after ~10s

# Step 3: Health check
curl http://localhost:8080/healthz
# Expected: JSON response with health status

# Step 4: UI accessibility
open http://localhost:3200
# OR: curl -s http://localhost:3200 | head -20
# Expected: HTML page with AdapterOS dashboard

# Step 5: API check
curl http://localhost:8080/v1/meta
# Expected: JSON metadata response
```

**Expected Results:**
- launch.sh executes without errors
- Backend listening on 8080
- UI listening on 3200
- Health endpoints respond
- Dashboard accessible at http://localhost:3200

---

### Phase 3: Service Management

**Objective:** Test individual service controls

```bash
# Step 1: Start only backend
./launch.sh backend
# Expected: Backend starts on 8080, no UI

# Step 2: Start with MLX backend
./launch.sh backend mlx ./models/qwen2.5-7b-mlx
# Expected: Backend starts with AOS_MLX_FFI_MODEL environment variable set

# Step 3: Check service status
./scripts/service-manager.sh status
# Expected: Output showing which services are running (backend/ui/menu-bar)

# Step 4: Stop services gracefully
./scripts/service-manager.sh stop all graceful
# Expected: Services shutdown cleanly with messages

# Step 5: Verify ports are free after stop
lsof -i :8080 && echo "⚠️  Still running" || echo "✅ Port freed"
lsof -i :3200 && echo "⚠️  Still running" || echo "✅ Port freed"
```

**Expected Results:**
- Backend-only mode works
- MLX backend option accepted
- Status command shows service state
- Graceful shutdown completes
- Ports freed after shutdown

---

### Phase 4: Alternative Launcher (start.sh)

**Objective:** Verify alternative startup method

```bash
# Step 1: Start with start.sh instead
./scripts/start.sh
# Expected: Backend on 8080, UI on 5173 (Vite default)

# Step 2: Verify UI port difference
lsof -i :5173
# Expected: Node process listening (Vite dev server)

# Step 3: Access UI on Vite port
open http://localhost:5173
# Expected: Same AdapterOS dashboard, via Vite dev server

# Step 4: Stop and compare
# Ctrl+C to stop
# Expected: Both services shutdown
```

**Expected Results:**
- start.sh works as alternative
- Uses standard Vite port (5173)
- Backend still on 8080
- Simpler startup process

---

## Troubleshooting Guide

### Issue: "Port 8080 already in use"

```bash
# Identify process using 8080
lsof -i :8080
# Kill the process
kill -9 <PID>
# Or use launch.sh's built-in resolution (it will try to kill AdapterOS processes)
./launch.sh  # Will attempt to clean up automatically
```

### Issue: "service-manager.sh not found"

```bash
# Verify it exists
ls -la scripts/service-manager.sh
# If missing, it was created by the agent - check if creation failed
# Re-run the agent to create it
```

### Issue: "Backend won't start"

```bash
# Check config file
cat configs/cp.toml | head -20
# Verify database path
mkdir -p var
# Try building backend directly
cargo build -p adapteros-server
# Check for errors in build output
```

### Issue: "UI won't start"

```bash
# Check Node/pnpm availability
which pnpm npm node
# Install dependencies if needed
cd ui && pnpm install
# Try starting Vite directly
pnpm dev
```

### Issue: "Health check fails"

```bash
# Check if processes are running
ps aux | grep adapteros-server
ps aux | grep vite
# Check logs
tail -50 var/logs/backend.log  # If available
# Try direct connection
nc -zv localhost 8080
```

---

## Verification Checklist

After completing all phases, verify:

- [ ] **Pre-flight checks pass** - All infrastructure components detected
- [ ] **Backend starts** - Process running on port 8080
- [ ] **Backend responds** - `/healthz` endpoint works
- [ ] **UI starts** - Process running on port 3200 (or 5173 for start.sh)
- [ ] **Dashboard accessible** - Can load http://localhost:3200 in browser
- [ ] **API accessible** - Can call `/v1/meta` and other endpoints
- [ ] **Service manager works** - Can start/stop individual services
- [ ] **Graceful shutdown works** - Ports freed after Ctrl+C
- [ ] **Alternative launcher works** - start.sh executes successfully
- [ ] **No residual processes** - All ports free after shutdown

---

## Expected Behavior Summary

| Action | Expected | Actual | Status |
|--------|----------|--------|--------|
| Run `./launch.sh` | Both services start, health checks pass | | |
| Access http://localhost:8080/healthz | JSON health response | | |
| Access http://localhost:3200 | Dashboard loads | | |
| Run `./launch.sh status` | Shows running services | | |
| Press Ctrl+C | Clean shutdown, ports freed | | |
| Run `./scripts/start.sh` | Services start on 8080/5173 | | |

---

## Notes

- **First-time startup** may take longer (cargo build, pnpm install)
- **Subsequent runs** should be faster (cached dependencies)
- **Port conflicts** are handled automatically by launch.sh
- **Database initialization** happens on first run
- **Configuration** can be overridden via `AOS_MLX_FFI_MODEL` and `DATABASE_URL` environment variables

---

## Sign-off

**Created:** 2025-11-22
**Test Environment:** macOS 13.x (Darwin)
**Prerequisites:** ✅ Verified
**Ready for Testing:** ✅ Yes

---
