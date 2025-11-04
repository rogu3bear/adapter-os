# Launch Script Safety & Validation Improvements

## Overview
This document describes all the corners that were cut in the initial launch script implementation and how they were fixed. These improvements make the launch system more robust, safer, and provide better error visibility.

## Improvements Made

### 1. ✅ Safe Port Process Killing

**Problem**: Originally killed ANY process on ports 3300/3200, risking unrelated services.

**Fix**: 
- Filters processes to only AdapterOS-related ones (adapteros, node, pnpm, vite, react)
- Refuses to kill non-AdapterOS processes automatically
- Shows process command names when killing
- Uses graceful TERM signal first, then KILL only if needed

**Impact**: Won't accidentally kill databases, other services, or unrelated development tools.

---

### 2. ✅ Proper Health Check Validation

**Problem**: Accepting "maybe it's running" as validation, multiple fallbacks hiding real failures.

**Fix**:
- Proper HTTP endpoint verification (`/v1/meta`, then `/healthz`)
- Checks actual port binding with `lsof`
- Validates process existence AND HTTP responsiveness
- Clear warnings when process runs but HTTP fails
- Extracts actual error messages from logs

**Impact**: Users know immediately if backend is actually working or just appears to be running.

---

### 3. ✅ Better Process Validation

**Problem**: Just checked if PID exists after 3 seconds - too simplistic.

**Fix**:
- 10-second validation loop (1s intervals)
- Checks process existence AND port binding
- Extracts error messages from `server.log` when process dies
- Shows actual crash reasons: "Backend server crashed: [error message]"
- Handles slow initialization gracefully

**Impact**: Better understanding of why startup fails, faster failure detection.

---

### 4. ✅ Build Verification

**Problem**: Assumed build succeeded if command returned 0, didn't verify binary exists.

**Fix**:
- Shows filtered build output (Compiling/Finished/errors)
- Verifies `target/debug/adapteros-server` actually exists after build
- Fails fast if build completed but binary missing
- Clear error messages with next steps

**Impact**: Catches build issues immediately instead of failing later at startup.

---

### 5. ✅ Dependency Checks

**Problem**: Assumed pnpm/npm exist, no warnings if missing.

**Fix**:
- Checks for `pnpm` or `npm` before starting UI
- Warns if missing (doesn't fail - UI just won't start)
- Database directory check and creation
- Validates project structure (configs/cp.toml exists)

**Impact**: Clear warnings about missing dependencies instead of cryptic failures.

---

### 6. ✅ Better Error Messages

**Problem**: Generic "check server.log" without extracting useful info.

**Fix**:
- Extracts actual error lines from logs (`grep -iE "(error|panic|fatal)"`)
- Shows specific error messages in output
- Provides actionable commands: "tail -20 server.log"
- Distinguishes between different failure modes

**Impact**: Users can diagnose issues faster without manually checking logs.

---

### 7. ✅ Improved Cleanup Handling

**Problem**: Simple trap handler, only INT signal.

**Fix**:
- Dedicated `cleanup_and_exit()` function
- Handles both INT (Ctrl+C) and TERM signals
- Better status display during periodic checks
- Clear separation between startup and runtime

**Impact**: More graceful shutdown, handles various termination scenarios.

---

### 8. ✅ Port Conflict Resolution

**Problem**: Just warned about conflicts, didn't resolve them.

**Fix**:
- Actively frees ports by killing AdapterOS processes
- Validates port is actually free after killing
- Clear status messages during port cleanup
- Fails fast if port cannot be freed

**Impact**: Launch script handles common "port in use" scenario automatically.

---

## Code Changes Summary

### `launch.sh` Changes:
- Added `kill_port_processes()` function with safety filtering
- Enhanced `wait_for_service()` usage with proper fallbacks
- Improved build verification with binary existence check
- Added dependency checks (pnpm/npm, database)
- Better error extraction and reporting
- Improved cleanup with dedicated function
- Enhanced status display formatting

### `scripts/service-manager.sh` Changes:
- Enhanced `backend_start()` with proper validation loop
- Port binding verification (not just PID check)
- Error extraction from logs on process death
- Handles slow initialization gracefully

---

## Testing Recommendations

1. **Port Conflict**: Run two launch scripts simultaneously - should handle gracefully
2. **Missing Dependencies**: Run without pnpm/npm - should warn but continue
3. **Build Failure**: Break build temporarily - should show clear error
4. **Process Crash**: Kill backend manually - should detect and report properly
5. **Slow Startup**: Add delay to backend - should wait appropriately
6. **Non-AdapterOS Port Usage**: Put unrelated service on 3300 - should refuse to kill

---

## Safety Guarantees

✅ **Will NOT kill**: Databases, unrelated services, non-AdapterOS processes  
✅ **Will kill safely**: Only AdapterOS processes (adapteros-server, pnpm dev, vite)  
✅ **Clear warnings**: When processes can't be identified or freed  
✅ **Graceful degradation**: Continues with partial failure when appropriate  
✅ **Actionable errors**: Provides specific commands and locations for debugging

---

## Migration Notes

If you have existing launch scripts, these improvements are:
- **Backward compatible**: Existing usage patterns still work
- **Additive**: Only adds safety checks and better validation
- **Non-breaking**: Doesn't change command-line interface

Apply this patch with:
```bash
# Review the changes
cat launch_improvements.patch

# Apply manually (files are new, so patch may not apply directly)
# Instead, review and manually incorporate the improvements
```

---

## Future Improvements

Potential enhancements:
- [ ] Configurable port detection patterns
- [ ] Service health check endpoints
- [ ] Automatic database migration on first run
- [ ] Service dependency graph validation
- [ ] Metrics collection during launch
- [ ] Dry-run mode to preview actions

