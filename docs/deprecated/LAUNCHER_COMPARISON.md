# AdapterOS Launcher Comparison

This document compares the available launcher scripts for starting AdapterOS.

## Quick Summary

| Feature | `start.sh` | `launch.sh` | `run_complete_system.sh` |
|---------|------------|-------------|--------------------------|
| **Location** | `scripts/start.sh` | `launch.sh` (root) | `scripts/run_complete_system.sh` |
| **Complexity** | Simple | Full-featured | Comprehensive |
| **Backend Port** | 8080 | 3300 | 8080 |
| **UI Port** | 5173 (Vite default) | 3200 | 5173 (Vite default) |
| **Service Manager** | No | Yes | No |
| **System Checks** | Basic | Moderate | Extensive |
| **Health Monitoring** | Startup only | Continuous (30s) | Startup only |
| **macOS Menu Bar** | No | Yes | No |
| **Browser Auto-open** | No | No | Yes (optional) |

---

## scripts/start.sh (Recommended for Development)

**Purpose:** Simple, direct startup script for developers.

### Ports
- **Backend API:** 8080
- **UI (Vite):** 5173

### What It Does
1. Checks for models in `models/` directory
2. Runs database migrations if database does not exist
3. Builds backend (`adapteros-server`) if binary not found
4. Installs UI dependencies via pnpm if needed
5. Starts backend server with `--config configs/cp.toml`
6. Starts UI via `pnpm dev`
7. Waits for backend health check (20s timeout)
8. Graceful shutdown on Ctrl+C

### Options
```bash
./scripts/start.sh                 # Full system (backend + UI)
./scripts/start.sh --backend-only  # Backend only
./scripts/start.sh --help          # Show help
```

### Pros
- Simple and predictable
- Direct execution (no service manager dependencies)
- Standard Vite/Cargo patterns
- Logs to `/tmp/aos-server.log` and `/tmp/aos-ui.log`
- Uses release build by default

### Cons
- No continuous health monitoring
- No automatic port conflict resolution
- No menu bar integration

### Log Locations
- Backend: `/tmp/aos-server.log`
- UI: `/tmp/aos-ui.log`

---

## launch.sh (Full-Featured Production Launcher)

**Purpose:** Full system launcher with service management, health monitoring, and macOS integration.

### Ports
- **Backend API:** 3300
- **UI:** 3200

### What It Does
1. Pre-flight checks (project directory, build, database)
2. Port conflict detection with automatic process killing (AdapterOS processes only)
3. Starts services via `scripts/service-manager.sh`
4. Waits for service readiness with HTTP health checks
5. Starts macOS menu bar app (optional, macOS only)
6. Continuous status monitoring every 30 seconds
7. Graceful shutdown via `scripts/graceful-shutdown.sh`

### Options
```bash
./launch.sh                        # Full system launch
./launch.sh backend                # Backend only (Metal backend)
./launch.sh backend mlx <path>     # Backend with MLX model path
./launch.sh ui                     # Backend + UI only
./launch.sh status                 # Show service status
./launch.sh stop [mode]            # Stop (graceful|fast|immediate)
./launch.sh help                   # Show help
```

### Dependencies
- `scripts/service-manager.sh` - Required for service lifecycle
- `scripts/graceful-shutdown.sh` - Used for cleanup

### Pros
- Service management abstraction
- Automatic port conflict resolution
- Continuous health monitoring (30-second intervals)
- macOS menu bar integration
- Multiple startup modes (backend, ui, full)
- Graceful shutdown with multiple modes

### Cons
- More complex, depends on additional scripts
- Uses debug build by default (slower)
- Different ports than standard configuration (3300/3200 vs 8080/5173)

### Log Locations
- Backend: `server.log` (project root)
- Controlled by service-manager.sh

---

## scripts/run_complete_system.sh (Full System with Extensive Checks)

**Purpose:** Comprehensive startup with detailed system requirements validation.

### Ports
- **Backend API:** 8080
- **UI (Vite):** 5173

### What It Does
1. **System requirements check:**
   - macOS platform verification
   - Apple Silicon chip detection
   - Memory check (16GB min, 48GB recommended)
   - macOS version check (14.0+ recommended)
   - Rust toolchain verification
   - Node.js and pnpm verification
2. **Model validation:**
   - Model directory existence
   - Required files check (config.json, tokenizer.json)
   - Weight file verification
   - Model size reporting
3. **Database setup:**
   - Creates required directories (var/artifacts, var/bundles, var/alerts)
   - Runs migrations via adapteros-orchestrator
4. **Build check:**
   - Builds release binary if needed
5. **Port check:**
   - Interactive prompt to kill conflicting processes
6. **Service startup:**
   - Starts API server via `cargo run --release -p adapteros-server-api`
   - Starts UI via `pnpm dev`
   - Auto-opens browser to dashboard (optional)
7. **Summary output:**
   - Service URLs
   - Model info
   - Example curl commands
   - Performance expectations

### Options
```bash
./scripts/run_complete_system.sh              # Full startup
./scripts/run_complete_system.sh --no-ui      # Backend only
./scripts/run_complete_system.sh --no-browser # No auto browser open
./scripts/run_complete_system.sh --help       # Show help
```

### Environment Variables
- `AOS_MLX_FFI_MODEL` - Model directory path
- `DATABASE_URL` - SQLite database URL
- `RUST_LOG` - Log level (default: info)

### Pros
- Most comprehensive system validation
- Detailed hardware requirements checking
- Model file validation
- Example commands in output
- Performance expectations displayed
- Interactive port conflict resolution
- Auto-opens browser

### Cons
- Most verbose and slowest startup
- Interactive prompts may block automation
- Requires Apple Silicon (fails on Intel Macs)

### Log Locations
- Backend: `/tmp/adapteros-server.log`
- UI: `/tmp/adapteros-ui.log`

---

## Recommendations

### For Development
Use `scripts/start.sh`:
```bash
./scripts/start.sh
```
Simple, direct, uses standard ports (8080/5173).

### For Production-like Testing
Use `scripts/run_complete_system.sh`:
```bash
./scripts/run_complete_system.sh --no-browser
```
Full validation, ensures system meets requirements.

### For Service Management
Use `launch.sh` if service-manager.sh is available:
```bash
./launch.sh
```
Full service lifecycle management with monitoring.

---

## Port Configuration Reference

| Script | Backend | UI | Config Used |
|--------|---------|-----|-------------|
| `start.sh` | 8080 | 5173 | `configs/cp.toml` |
| `launch.sh` | 3300 | 3200 | Via service-manager |
| `run_complete_system.sh` | 8080 | 5173 | Via cargo run |

**Note:** The standard configuration uses ports 8080 (backend) and 5173 (UI/Vite). The `launch.sh` script uses non-standard ports (3300/3200) via its service manager.

---

## Troubleshooting

### start.sh fails to start backend
1. Check logs: `tail -f /tmp/aos-server.log`
2. Verify database exists: `ls -la var/aos-cp.sqlite3`
3. Build manually: `cargo build --release -p adapteros-server`

### launch.sh can't find service-manager.sh
The script depends on `scripts/service-manager.sh`. Verify it exists or use `start.sh` instead.

### run_complete_system.sh fails system checks
This script requires Apple Silicon Mac. For Intel Macs, use `start.sh`.

### Port conflicts
- `start.sh`: Manually kill processes on port 8080/5173
- `launch.sh`: Automatic (AdapterOS processes only)
- `run_complete_system.sh`: Interactive prompt

---

*Last updated: 2025-11-22*
